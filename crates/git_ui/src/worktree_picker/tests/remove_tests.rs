use super::*;

#[gpui::test]
async fn test_remove_open_worktree_workspace_from_window(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                ".git": {},
                "file.txt": "buffer_text",
            },
            "worktrees": {},
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/root/project/.git").as_ref(),
        &[("file.txt", "buffer_text".to_string())],
        "deadbeef",
    );

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let worktree_path = PathBuf::from(path!("/root/worktrees/open-wt"));
    cx.update(|cx| {
        repository.update(cx, |repository, _| {
            repository.create_worktree(
                git::repository::CreateWorktreeTarget::NewBranch {
                    branch_name: "open-wt".to_string(),
                    base_sha: Some("deadbeef".to_string()),
                },
                worktree_path.clone(),
            )
        })
    })
    .await
    .unwrap()
    .unwrap();

    let worktree_project = Project::test(fs.clone(), [worktree_path.as_path()], cx).await;
    cx.executor().run_until_parked();

    let main_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));
    let worktree_group_key =
        worktree_project.read_with(cx, |project, cx| project.project_group_key(cx));
    assert_eq!(
        main_group_key, worktree_group_key,
        "the worktree workspace should belong to the same project group as the main repo"
    );

    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    let worktree_workspace = window_handle
        .update(cx, |multi_workspace, window, cx| {
            let worktree_workspace =
                cx.new(|cx| Workspace::test_new(worktree_project.clone(), window, cx));
            multi_workspace.add(worktree_workspace.clone(), window, cx);
            worktree_workspace
        })
        .unwrap();

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);
    let worktree_picker = cx.update(|window, cx| {
        cx.new(|cx| WorktreePicker::new(project, workspace.downgrade(), window, cx))
    });
    cx.run_until_parked();

    worktree_picker.update(&mut cx, |worktree_picker, cx| {
        worktree_picker.picker.update(cx, |picker, _| {
            assert!(
                picker
                    .delegate
                    .project_worktree_paths
                    .contains(&worktree_path),
                "the worktree should be considered open in this window"
            );
        })
    });

    worktree_picker.update_in(&mut cx, |worktree_picker, window, cx| {
        worktree_picker.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .remove_worktree_from_window(&worktree_path, window, cx);
        })
    });
    cx.run_until_parked();

    window_handle
        .read_with(&cx, |multi_workspace, _| {
            assert!(
                multi_workspace
                    .workspaces()
                    .all(|workspace| *workspace != worktree_workspace),
                "the worktree workspace should be removed from the window"
            );
        })
        .unwrap();

    worktree_picker.update(&mut cx, |worktree_picker, cx| {
        worktree_picker.picker.update(cx, |picker, _| {
            assert!(
                !picker
                    .delegate
                    .project_worktree_paths
                    .contains(&worktree_path),
                "the worktree should no longer be considered open in this window"
            );
        })
    });

    assert!(
        repo_contains_worktree(&repository, &worktree_path, &mut cx).await,
        "removing the worktree from the window should not delete the git worktree"
    );
}
