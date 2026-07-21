use super::*;

#[gpui::test]
async fn test_view_file_tracked(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "tracked": "tracked\n",
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/project/.git").as_ref(),
        &[("tracked", "old tracked\n".into())],
    );

    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    let panel = workspace.update_in(&mut cx, GitPanel::new);
    await_git_panel_entries(&panel, &mut cx).await;

    let entry_index = panel
        .read_with(&cx, |panel, _| {
            entry_index_for_repo_path(panel, &repo_path("tracked"))
        })
        .expect("tracked file should exist in the changes list");

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.selected_entry = Some(entry_index);
        panel.view_file(&ViewFile, window, cx);
    });
    cx.run_until_parked();

    assert_editor_opened_with_path(&workspace, Path::new("tracked"), &mut cx);
}

#[gpui::test]
async fn test_view_file_untracked(cx: &mut TestAppContext) {
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

    let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

    cx.read(|cx| {
        project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .as_local()
            .unwrap()
            .scan_complete()
    })
    .await;

    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().sort_by = Some(GitPanelSortBy::Path);
            })
        });
    });

    let panel = workspace.update_in(&mut cx, GitPanel::new);
    await_git_panel_entries(&panel, &mut cx).await;

    let entry_index = panel
        .read_with(&cx, |panel, _| {
            entry_index_for_repo_path(panel, &repo_path("untracked"))
        })
        .expect("untracked file should exist in the changes list");

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.selected_entry = Some(entry_index);
        panel.view_file(&ViewFile, window, cx);
    });
    cx.run_until_parked();

    assert_editor_opened_with_path(&workspace, Path::new("untracked"), &mut cx);
}

#[gpui::test]
async fn test_view_file_tree_view(cx: &mut TestAppContext) {
    init_test(cx);

    let (_project, workspace, panel, mut cx) = setup_git_panel_with_changes(
        cx,
        json!({
            ".git": {},
            "src": {
                "a": {
                    "foo.rs": "fn foo() {}",
                },
            },
        }),
        &[("src/a/foo.rs", StatusCode::Modified)],
    )
    .await;

    cx.update(|_window, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git_panel.get_or_insert_default().tree_view = Some(true);
            })
        });
    });
    await_git_panel_entries(&panel, &mut cx).await;

    let entry_index = panel
        .read_with(&cx, |panel, _| {
            entry_index_for_repo_path(panel, &repo_path("src/a/foo.rs"))
        })
        .expect("foo.rs should exist in the tree view changes list");

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.selected_entry = Some(entry_index);
        panel.view_file(&ViewFile, window, cx);
    });
    cx.run_until_parked();

    assert_editor_opened_with_path(&workspace, Path::new("src/a/foo.rs"), &mut cx);
}

#[test]
fn test_format_git_error_toast_message_prefers_raw_rpc_message() {
    let rpc_error = RpcError::from_proto(
        &proto::Error {
            message: "Your local changes to the following files would be overwritten by merge\n"
                .to_string(),
            code: proto::ErrorCode::Internal as i32,
            tags: Default::default(),
        },
        "Pull",
    );

    let message = format_git_error_toast_message(&rpc_error);
    assert_eq!(
        message,
        "Your local changes to the following files would be overwritten by merge"
    );
}

#[test]
fn test_format_git_error_toast_message_prefers_raw_rpc_message_when_wrapped() {
    let rpc_error = RpcError::from_proto(
        &proto::Error {
            message: "Your local changes to the following files would be overwritten by merge\n"
                .to_string(),
            code: proto::ErrorCode::Internal as i32,
            tags: Default::default(),
        },
        "Pull",
    );
    let wrapped = rpc_error.context("sending pull request");

    let message = format_git_error_toast_message(&wrapped);
    assert_eq!(
        message,
        "Your local changes to the following files would be overwritten by merge"
    );
}
