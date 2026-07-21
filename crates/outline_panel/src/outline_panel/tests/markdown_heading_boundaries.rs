use super::*;

#[gpui::test]
async fn test_markdown_outline_selection_at_heading_boundaries(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/test",
        json!({
            "doc.md": indoc!("
                # Section A

                ## Sub Section A

                ## Sub Section B

                # Section B

            ")
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new("/test")], cx).await;
    project.read_with(cx, |project, _| project.languages().add(markdown_lang()));
    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);
    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.set_active(true, window, cx)
    });

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/test/doc.md"),
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx.run_until_parked();

    outline_panel.update_in(cx, |panel, window, cx| {
        panel.update_non_fs_items(window, cx);
        panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
    });

    // Helper function to move the cursor to the first column of a given row
    // and return the selected outline entry's text.
    let move_cursor_and_get_selection = |row: u32,
                                         cx: &mut VisualTestContext|
     -> Option<SharedString> {
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_ranges(Some(
                        language::Point::new(row, 0)..language::Point::new(row, 0),
                    ))
                });
            });
        });

        cx.run_until_parked();

        outline_panel.read_with(cx, |panel, _cx| {
            panel.selected_entry().and_then(|entry| match entry {
                PanelEntry::Outline(OutlineEntry::Outline(outline)) => Some(outline.text.clone()),
                _ => None,
            })
        })
    };

    assert_eq!(
        move_cursor_and_get_selection(0, cx).as_deref(),
        Some("# Section A"),
        "Cursor at row 0 should select '# Section A'"
    );

    assert_eq!(
        move_cursor_and_get_selection(2, cx).as_deref(),
        Some("## Sub Section A"),
        "Cursor at row 2 should select '## Sub Section A'"
    );

    assert_eq!(
        move_cursor_and_get_selection(4, cx).as_deref(),
        Some("## Sub Section B"),
        "Cursor at row 4 should select '## Sub Section B'"
    );

    assert_eq!(
        move_cursor_and_get_selection(6, cx).as_deref(),
        Some("# Section B"),
        "Cursor at row 6 should select '# Section B'"
    );
}
