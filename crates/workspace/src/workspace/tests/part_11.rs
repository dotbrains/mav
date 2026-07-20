use super::*;

#[gpui::test]
async fn test_most_recent_active_path_skips_read_only_paths(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "src": { "main.py": "" },
            ".venv": { "lib": { "dep.py": "" } },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    // Configure .venv as read-only
    workspace.update_in(cx, |_workspace, _window, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_user_settings(r#"{"read_only_files": ["**/.venv/**"]}"#, cx)
                .ok();
        });
    });

    let item_dep = cx.new(|cx| {
        TestItem::new(cx).with_project_items(&[TestProjectItem::new_in_worktree(
            1001,
            ".venv/lib/dep.py",
            worktree_id,
            cx,
        )])
    });

    // dep.py is active but matches read_only_files → should be skipped
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_dep.clone()), None, true, window, cx);
    });
    let path = workspace.read_with(cx, |workspace, cx| workspace.most_recent_active_path(cx));
    assert_eq!(path, None);
}

#[gpui::test]
async fn test_open_url_or_file_routes_urls(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file.txt": "content",
            "subdir": {
                "nested.txt": "nested content"
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // Test opening an HTTPS URL - should go to open_url
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_url_or_file("https://example.com", None, window, cx);
    });
    assert_eq!(cx.opened_url(), Some("https://example.com".to_string()));

    // Test opening an HTTP URL - should go to open_url
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_url_or_file("http://example.com", None, window, cx);
    });
    assert_eq!(cx.opened_url(), Some("http://example.com".to_string()));

    // Test opening other URI schemes - should go to open_url
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_url_or_file("mailto:test@example.com", None, window, cx);
    });
    assert_eq!(cx.opened_url(), Some("mailto:test@example.com".to_string()));

    // Test opening a path that doesn't exist and doesn't match project - should fallback to open_url
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_url_or_file("nonexistent.txt", None, window, cx);
    });
    assert_eq!(cx.opened_url(), Some("nonexistent.txt".to_string()));
}
