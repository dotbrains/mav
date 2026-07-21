use super::*;

#[gpui::test]
async fn test_buffer_search(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/test",
        json!({
            "foo.txt": r#"<_constitution>

</_constitution>



## 📊 Output

| Field          | Meaning                |
"#
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/test/foo.txt"),
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..OpenOptions::default()
                },
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let search_bar = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| {
            let mut search_bar = BufferSearchBar::new(None, window, cx);
            search_bar.set_active_pane_item(Some(&editor), window, cx);
            search_bar.show(window, cx);
            search_bar
        })
    });

    let outline_panel = outline_panel(&workspace, cx);

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.set_active(true, window, cx)
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("  ", None, true, window, cx)
        })
        .await
        .unwrap();

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            "search: | Field«  »        | Meaning                |  <==== selected
search: | Field  «  »      | Meaning                |
search: | Field    «  »    | Meaning                |
search: | Field      «  »  | Meaning                |
search: | Field        «  »| Meaning                |
search: | Field          | Meaning«  »              |
search: | Field          | Meaning  «  »            |
search: | Field          | Meaning    «  »          |
search: | Field          | Meaning      «  »        |
search: | Field          | Meaning        «  »      |
search: | Field          | Meaning          «  »    |
search: | Field          | Meaning            «  »  |
search: | Field          | Meaning              «  »|"
        );
    });
}
