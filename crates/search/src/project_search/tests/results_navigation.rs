use super::*;

#[gpui::test]
async fn test_project_search(cx: &mut TestAppContext) {
    fn dp(row: u32, col: u32) -> DisplayPoint {
        DisplayPoint::new(DisplayRow(row), col)
    }
    fn assert_active_match_index(
        search_view: &WindowHandle<ProjectSearchView>,
        cx: &mut TestAppContext,
        expected_index: usize,
    ) {
        search_view
            .update(cx, |search_view, _window, _cx| {
                assert_eq!(search_view.active_match_index, Some(expected_index));
            })
            .unwrap();
    }

    fn assert_selection_range(
        search_view: &WindowHandle<ProjectSearchView>,
        cx: &mut TestAppContext,
        expected_range: Range<DisplayPoint>,
    ) {
        search_view
            .update(cx, |search_view, _window, cx| {
                assert_eq!(
                    search_view.results_editor.update(cx, |editor, cx| editor
                        .selections
                        .display_ranges(&editor.display_snapshot(cx))),
                    [expected_range]
                );
            })
            .unwrap();
    }

    fn assert_highlights(
        search_view: &WindowHandle<ProjectSearchView>,
        cx: &mut TestAppContext,
        expected_highlights: Vec<(Range<DisplayPoint>, &str)>,
    ) {
        search_view
            .update(cx, |search_view, window, cx| {
                let match_bg = cx.theme().colors().search_match_background;
                let active_match_bg = cx.theme().colors().search_active_match_background;
                let selection_bg = cx
                    .theme()
                    .colors()
                    .editor_document_highlight_bracket_background;

                let highlights: Vec<_> = expected_highlights
                    .into_iter()
                    .map(|(range, color_type)| {
                        let color = match color_type {
                            "active" => active_match_bg,
                            "match" => match_bg,
                            "selection" => selection_bg,
                            _ => panic!("Unknown color type"),
                        };
                        (range, color)
                    })
                    .collect();

                assert_eq!(
                    search_view.results_editor.update(cx, |editor, cx| editor
                        .all_text_background_highlights(window, cx)),
                    highlights.as_slice()
                );
            })
            .unwrap();
    }

    fn select_match(
        search_view: &WindowHandle<ProjectSearchView>,
        cx: &mut TestAppContext,
        direction: Direction,
    ) {
        search_view
            .update(cx, |search_view, window, cx| {
                search_view.select_match(direction, window, cx);
            })
            .unwrap();
    }

    init_test(cx);

    // Override active search match color since the fallback theme uses the same color
    // for normal search match and active one, which can make this test less robust.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.theme.experimental_theme_overrides = Some(ThemeStyleContent {
                    colors: ThemeColorsContent {
                        search_active_match_background: Some("#ff0000ff".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
            "four.rs": "const FOUR: usize = one::ONE + three::THREE;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    perform_search(search_view, "TWO", cx);
    cx.run_until_parked();

    search_view
            .update(cx, |search_view, _window, cx| {
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst THREE: usize = one::ONE + two::TWO;\n\n\nconst TWO: usize = one::ONE + one::ONE;"
                );
            })
            .unwrap();

    assert_active_match_index(&search_view, cx, 0);
    assert_selection_range(&search_view, cx, dp(2, 32)..dp(2, 35));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "active"),
            (dp(2, 37)..dp(2, 40), "selection"),
            (dp(2, 37)..dp(2, 40), "match"),
            (dp(5, 6)..dp(5, 9), "selection"),
            (dp(5, 6)..dp(5, 9), "match"),
        ],
    );
    select_match(&search_view, cx, Direction::Next);
    cx.run_until_parked();

    assert_active_match_index(&search_view, cx, 1);
    assert_selection_range(&search_view, cx, dp(2, 37)..dp(2, 40));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "selection"),
            (dp(2, 32)..dp(2, 35), "match"),
            (dp(2, 37)..dp(2, 40), "active"),
            (dp(5, 6)..dp(5, 9), "selection"),
            (dp(5, 6)..dp(5, 9), "match"),
        ],
    );
    select_match(&search_view, cx, Direction::Next);
    cx.run_until_parked();

    assert_active_match_index(&search_view, cx, 2);
    assert_selection_range(&search_view, cx, dp(5, 6)..dp(5, 9));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "selection"),
            (dp(2, 32)..dp(2, 35), "match"),
            (dp(2, 37)..dp(2, 40), "selection"),
            (dp(2, 37)..dp(2, 40), "match"),
            (dp(5, 6)..dp(5, 9), "active"),
        ],
    );
    select_match(&search_view, cx, Direction::Next);
    cx.run_until_parked();

    assert_active_match_index(&search_view, cx, 0);
    assert_selection_range(&search_view, cx, dp(2, 32)..dp(2, 35));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "active"),
            (dp(2, 37)..dp(2, 40), "selection"),
            (dp(2, 37)..dp(2, 40), "match"),
            (dp(5, 6)..dp(5, 9), "selection"),
            (dp(5, 6)..dp(5, 9), "match"),
        ],
    );
    select_match(&search_view, cx, Direction::Prev);
    cx.run_until_parked();

    assert_active_match_index(&search_view, cx, 2);
    assert_selection_range(&search_view, cx, dp(5, 6)..dp(5, 9));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "selection"),
            (dp(2, 32)..dp(2, 35), "match"),
            (dp(2, 37)..dp(2, 40), "selection"),
            (dp(2, 37)..dp(2, 40), "match"),
            (dp(5, 6)..dp(5, 9), "active"),
        ],
    );
    select_match(&search_view, cx, Direction::Prev);
    cx.run_until_parked();

    assert_active_match_index(&search_view, cx, 1);
    assert_selection_range(&search_view, cx, dp(2, 37)..dp(2, 40));
    assert_highlights(
        &search_view,
        cx,
        vec![
            (dp(2, 32)..dp(2, 35), "selection"),
            (dp(2, 32)..dp(2, 35), "match"),
            (dp(2, 37)..dp(2, 40), "active"),
            (dp(5, 6)..dp(5, 9), "selection"),
            (dp(5, 6)..dp(5, 9), "match"),
        ],
    );
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.fold_all(&FoldAll, window, cx);
            })
        })
        .expect("Should fold fine");
    cx.run_until_parked();

    let results_collapsed = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("got results_collapsed");

    assert!(results_collapsed);
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.unfold_all(&UnfoldAll, window, cx);
            })
        })
        .expect("Should unfold fine");
    cx.run_until_parked();

    let results_collapsed = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("got results_collapsed");

    assert!(!results_collapsed);
}
