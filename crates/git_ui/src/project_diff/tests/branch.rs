use super::*;

#[gpui::test]
async fn test_branch_diff(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "a.txt": "C",
            "b.txt": "new",
            "c.txt": "in-merge-base-and-work-tree",
            "d.txt": "created-in-head",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let diff = cx
        .update(|window, cx| {
            ProjectDiff::new_with_default_branch(project.clone(), workspace, window, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    fs.set_head_for_repo(
        Path::new(path!("/project/.git")),
        &[("a.txt", "B".into()), ("d.txt", "created-in-head".into())],
        "sha",
    );
    // fs.set_index_for_repo(dot_git, index_state);
    fs.set_merge_base_content_for_repo(
        Path::new(path!("/project/.git")),
        &[
            ("a.txt", "A".into()),
            ("c.txt", "in-merge-base-and-work-tree".into()),
        ],
    );
    cx.run_until_parked();

    let editor = diff.read_with(cx, |diff, cx| diff.editor.read(cx).rhs_editor().clone());

    assert_state_with_diff(
        &editor,
        cx,
        &"
                - A
                + ˇC
                + new
                + created-in-head"
            .unindent(),
    );

    let statuses: HashMap<Arc<RelPath>, Option<FileStatus>> = editor.update(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .all_buffers()
            .iter()
            .map(|buffer| {
                (
                    buffer.read(cx).file().unwrap().path().clone(),
                    editor.status_for_buffer_id(buffer.read(cx).remote_id(), cx),
                )
            })
            .collect()
    });

    assert_eq!(
        statuses,
        HashMap::from_iter([
            (
                rel_path("a.txt").into_arc(),
                Some(FileStatus::Tracked(TrackedStatus {
                    index_status: git::status::StatusCode::Modified,
                    worktree_status: git::status::StatusCode::Modified
                }))
            ),
            (rel_path("b.txt").into_arc(), Some(FileStatus::Untracked)),
            (
                rel_path("d.txt").into_arc(),
                Some(FileStatus::Tracked(TrackedStatus {
                    index_status: git::status::StatusCode::Added,
                    worktree_status: git::status::StatusCode::Added
                }))
            )
        ])
    );
}

#[gpui::test]
async fn test_branch_diff_action_matches_existing_item_by_base_ref(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "a.txt": "changed",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let target_branch_diff = cx
        .update(|window, cx| {
            let Some(repository) = project.read(cx).active_repository(cx) else {
                return Task::ready(Err(anyhow!("No active repository")));
            };
            ProjectDiff::new_with_branch_base(
                project.clone(),
                workspace.clone(),
                "topic".into(),
                repository,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(
            Box::new(target_branch_diff.clone()),
            None,
            true,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(BranchDiff.boxed_clone(), cx);
    });
    cx.run_until_parked();

    let (active_base_ref, mut base_refs) = workspace.update(cx, |workspace, cx| {
        let active_item = workspace.active_item_as::<ProjectDiff>(cx).unwrap();
        let active_base_ref = match active_item.read(cx).diff_base(cx) {
            DiffBase::Merge { base_ref } => base_ref.to_string(),
            DiffBase::Head => panic!("expected active item to be a branch diff"),
        };
        let base_refs = workspace
            .items_of_type::<ProjectDiff>(cx)
            .filter_map(|item| match item.read(cx).diff_base(cx) {
                DiffBase::Merge { base_ref } => Some(base_ref.to_string()),
                DiffBase::Head => None,
            })
            .collect::<Vec<_>>();
        (active_base_ref, base_refs)
    });
    base_refs.sort();

    assert_eq!(active_base_ref, "origin/main");
    assert_eq!(base_refs, vec!["origin/main", "topic"]);
}
