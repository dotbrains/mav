#[gpui::test]
async fn test_delete_prompt_escapes_markdown_in_file_name(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "__somefile__": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    select_path(&panel, "root/__somefile__", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.delete(&Delete { skip_prompt: false }, window, cx)
    });
    let (message, _detail) = cx
        .pending_prompt()
        .expect("delete should show a confirmation prompt");

    assert_eq!(
        message,
        "Are you sure you want to permanently delete `__somefile__`?"
    );
}
