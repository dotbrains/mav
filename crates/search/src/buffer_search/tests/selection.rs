use super::*;

#[gpui::test]
async fn test_cmd_e_then_cmd_g_uses_selection_for_find(cx: &mut TestAppContext) {
    init_globals(cx);
    let app_state = cx.update(AppState::test);
    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let buffer = cx.new(|cx| {
        Buffer::local(
            r#"
            dad
            cat
            mom
            dog
            dog
            cat
            dad
            mom
            "#
            .unindent(),
            cx,
        )
    });
    let multibuffer = cx.update(|cx| MultiBuffer::build_from_buffer(buffer, cx));
    let mut editor = None;
    let mut search_bar = None;

    let window = cx.add_window(|window, cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-macos.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
        let workspace = cx.new(|cx| Workspace::test_new(project.clone(), window, cx));
        let multi_workspace = MultiWorkspace::new(workspace.clone(), window, cx);
        let buffer_search_bar = cx.new(|cx| BufferSearchBar::new(None, window, cx));
        workspace.update(cx, |workspace, cx| {
            workspace.active_pane().update(cx, |pane, cx| {
                pane.toolbar().update(cx, |toolbar, cx| {
                    toolbar.add_item(buffer_search_bar.clone(), window, cx);
                });
            });
        });
        let editor_handle = cx.new(|cx| {
            Editor::new(
                editor::EditorMode::full(),
                multibuffer.clone(),
                Some(project.clone()),
                window,
                cx,
            )
        });
        workspace.update(cx, |workspace, cx| {
            workspace.add_item_to_center(Box::new(editor_handle.clone()), window, cx);
        });
        window.focus(&editor_handle.focus_handle(cx), cx);
        search_bar = Some(buffer_search_bar);
        editor = Some(editor_handle);
        multi_workspace
    });
    let cx = VisualTestContext::from_window(*window, cx).into_mut();
    let editor = editor.unwrap();
    let search_bar = search_bar.unwrap();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 1)
            ]);
        });
    });

    cx.simulate_keystrokes("cmd-e");

    search_bar.read_with(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "dog");
        assert_eq!(search_bar.active_match_index, Some(0));
    });
    cx.read(|cx| {
        assert_eq!(
            cx.read_from_find_pasteboard().and_then(|item| item.text()),
            Some("dog".to_string())
        );
    });

    cx.simulate_keystrokes("cmd-g");
    assert_eq!(
        editor.update(cx, |editor, cx| editor
            .selections
            .display_ranges(&editor.display_snapshot(cx))),
        [DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(4), 3)]
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1)
            ]);
        });
    });

    cx.simulate_keystrokes("cmd-e");

    search_bar.read_with(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "cat");
        assert_eq!(search_bar.active_match_index, Some(0));
    });
    cx.read(|cx| {
        assert_eq!(
            cx.read_from_find_pasteboard().and_then(|item| item.text()),
            Some("cat".to_string())
        );
    });

    cx.simulate_keystrokes("cmd-g");
    assert_eq!(
        editor.update(cx, |editor, cx| editor
            .selections
            .display_ranges(&editor.display_snapshot(cx))),
        [DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 3)]
    );
}
#[gpui::test]
async fn test_find_matches_in_selections_singleton_buffer_multiple_selections(
    cx: &mut TestAppContext,
) {
    init_globals(cx);
    let buffer = cx.new(|cx| {
        Buffer::local(
            r#"
            aaa bbb aaa ccc
            aaa bbb aaa ccc
            aaa bbb aaa ccc
            aaa bbb aaa ccc
            aaa bbb aaa ccc
            aaa bbb aaa ccc
            "#
            .unindent(),
            cx,
        )
    });
    let cx = cx.add_empty_window();
    let editor =
        cx.new_window_entity(|window, cx| Editor::for_buffer(buffer.clone(), None, window, cx));

    let search_bar = cx.new_window_entity(|window, cx| {
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(vec![Point::new(1, 0)..Point::new(2, 4)])
        })
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        let deploy = Deploy {
            focus: true,
            replace_enabled: false,
            selection_search_enabled: true,
        };
        search_bar.deploy(&deploy, None, window, cx);
    });

    cx.run_until_parked();

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("aaa", None, true, window, cx)
        })
        .await
        .unwrap();

    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.search_background_highlights(cx),
            &[
                Point::new(1, 0)..Point::new(1, 3),
                Point::new(1, 8)..Point::new(1, 11),
                Point::new(2, 0)..Point::new(2, 3),
            ]
        );
    });
}

#[perf]
#[gpui::test]
async fn test_find_matches_in_selections_multiple_excerpts_buffer_multiple_selections(
    cx: &mut TestAppContext,
) {
    init_globals(cx);
    let text = r#"
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc

        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        aaa bbb aaa ccc
        "#
    .unindent();

    let cx = cx.add_empty_window();
    let editor = cx.new_window_entity(|window, cx| {
        let multibuffer = MultiBuffer::build_multi(
            [
                (
                    &text,
                    vec![
                        Point::new(0, 0)..Point::new(2, 0),
                        Point::new(4, 0)..Point::new(5, 0),
                    ],
                ),
                (&text, vec![Point::new(9, 0)..Point::new(11, 0)]),
            ],
            cx,
        );
        Editor::for_multibuffer(multibuffer, None, window, cx)
    });

    let search_bar = cx.new_window_entity(|window, cx| {
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(vec![
                Point::new(1, 0)..Point::new(1, 4),
                Point::new(5, 3)..Point::new(6, 4),
            ])
        })
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        let deploy = Deploy {
            focus: true,
            replace_enabled: false,
            selection_search_enabled: true,
        };
        search_bar.deploy(&deploy, None, window, cx);
    });

    cx.run_until_parked();

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("aaa", None, true, window, cx)
        })
        .await
        .unwrap();

    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.search_background_highlights(cx),
            &[
                Point::new(1, 0)..Point::new(1, 3),
                Point::new(5, 8)..Point::new(5, 11),
                Point::new(6, 0)..Point::new(6, 3),
            ]
        );
    });
}
