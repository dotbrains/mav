use super::*;

#[gpui::test]
async fn test_open_diff(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "tracked": "tracked\n",
            "untracked": "\n",
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[("tracked", "old tracked\n".into())],
    );

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
    let panel = workspace.update_in(cx, GitPanel::new);

    // Disable status grouping and wait for entries to be updated,
    // as there should no longer be separators between Tracked and Untracked
    // files.
    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().group_by = Some(GitPanelGroupBy::None);
            })
        });
    });

    cx.update_window_entity(&panel, |panel, _, _| {
        std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
    })
    .await;

    // Confirm that `Open Diff` still works for the untracked file, updating
    // the Project Diff's active path.
    panel.update_in(cx, |panel, window, cx| {
        panel.selected_entry = Some(1);
        panel.open_diff(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    workspace.update_in(cx, |workspace, _window, cx| {
        let active_path = workspace
            .item_of_type::<ProjectDiff>(cx)
            .expect("ProjectDiff should exist")
            .read(cx)
            .active_project_path(cx)
            .expect("active_project_path should exist");

        assert_eq!(active_path.path, rel_path("untracked").into_arc());
    });
}

#[gpui::test]
async fn test_remote_operation_serialization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
    let panel = workspace.update_in(cx, GitPanel::new);

    panel.update(cx, |panel, cx| {
        // The first remote operation starts and records its kind, which the
        // button uses to render an "in progress" tooltip.
        assert!(panel.start_remote_operation(RemoteOperationKind::Fetch, cx));
        assert!(matches!(
            panel.pending_remote_operation,
            Some(RemoteOperationKind::Fetch)
        ));

        // A second remote operation is refused while one is pending, even a
        // different kind: we serialize all remote ops.
        assert!(!panel.start_remote_operation(RemoteOperationKind::Push, cx));

        // Clearing the pending operation re-opens the gate.
        panel.clear_remote_operation(cx);
        assert!(panel.pending_remote_operation.is_none());
        assert!(panel.start_remote_operation(RemoteOperationKind::Pull, cx));
    });
}
