use super::*;

#[gpui::test]
async fn test_update_on_uncommit(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "README.md": "# My cool project\n".to_owned()
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        Path::new(path!("/project/.git")),
        &[("README.md", "# My cool project\n".to_owned())],
    );
    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    cx.run_until_parked();

    let _editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("README.md")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(project_diff::Diff.boxed_clone(), cx);
    });
    cx.run_until_parked();
    let item = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<ProjectDiff>(cx).unwrap()
    });
    cx.focus(&item);
    let editor = item.read_with(cx, |item, cx| item.editor.read(cx).rhs_editor().clone());

    fs.set_head_and_index_for_repo(
        Path::new(path!("/project/.git")),
        &[(
            "README.md",
            "# My cool project\nDetails to come.\n".to_owned(),
        )],
    );
    cx.run_until_parked();

    let mut cx = EditorTestContext::for_editor_in(editor, cx).await;

    cx.assert_excerpts_with_selections("[EXCERPT]\nˇ# My cool project\nDetails to come.\n");
}

#[gpui::test]
async fn test_deploy_at_respects_active_repository_selection(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project_a"),
        json!({
            ".git": {},
            "a.txt": "CHANGED_A\n",
        }),
    )
    .await;
    fs.insert_tree(
        path!("/project_b"),
        json!({
            ".git": {},
            "b.txt": "CHANGED_B\n",
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        Path::new(path!("/project_a/.git")),
        &[("a.txt", "original_a\n".to_string())],
    );
    fs.set_head_and_index_for_repo(
        Path::new(path!("/project_b/.git")),
        &[("b.txt", "original_b\n".to_string())],
    );

    let project = Project::test(
        fs.clone(),
        [
            Path::new(path!("/project_a")),
            Path::new(path!("/project_b")),
        ],
        cx,
    )
    .await;

    let (worktree_a_id, worktree_b_id) = project.read_with(cx, |project, cx| {
        let mut worktrees: Vec<_> = project.worktrees(cx).collect();
        worktrees.sort_by_key(|w| w.read(cx).abs_path());
        (worktrees[0].read(cx).id(), worktrees[1].read(cx).id())
    });

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    cx.run_until_parked();

    // Select project A explicitly and open the diff.
    workspace.update(cx, |workspace, cx| {
        let git_store = workspace.project().read(cx).git_store().clone();
        git_store.update(cx, |git_store, cx| {
            git_store.set_active_repo_for_worktree(worktree_a_id, cx);
        });
    });
    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(project_diff::Diff.boxed_clone(), cx);
    });
    cx.run_until_parked();

    let diff_item = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<ProjectDiff>(cx).unwrap()
    });
    let paths_a = diff_item.read_with(cx, |diff, cx| diff.excerpt_paths(cx));
    assert_eq!(paths_a.len(), 1);
    assert_eq!(*paths_a[0], *"a.txt");

    // Switch the explicit active repository to project B and re-run the diff action.
    workspace.update(cx, |workspace, cx| {
        let git_store = workspace.project().read(cx).git_store().clone();
        git_store.update(cx, |git_store, cx| {
            git_store.set_active_repo_for_worktree(worktree_b_id, cx);
        });
    });
    cx.focus(&workspace);
    cx.update(|window, cx| {
        window.dispatch_action(project_diff::Diff.boxed_clone(), cx);
    });
    cx.run_until_parked();

    let same_diff_item = workspace.update(cx, |workspace, cx| {
        workspace.active_item_as::<ProjectDiff>(cx).unwrap()
    });
    assert_eq!(diff_item.entity_id(), same_diff_item.entity_id());

    let paths_b = diff_item.read_with(cx, |diff, cx| diff.excerpt_paths(cx));
    assert_eq!(paths_b.len(), 1);
    assert_eq!(*paths_b[0], *"b.txt");
}
