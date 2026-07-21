
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};

use super::*;
use editor::{DisplayPoint, display_map::DisplayRow};
use gpui::{Action, TestAppContext, VisualTestContext, WindowHandle};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use project::{FakeFs, Fs};
use serde_json::json;
use settings::{InlayHintSettingsContent, SettingsStore, ThemeColorsContent, ThemeStyleContent};
use util::{path, paths::PathStyle, rel_path::rel_path};
use util_macros::perf;
use workspace::{DeploySearch, MultiWorkspace};

#[perf]
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

#[perf]
#[gpui::test]
async fn test_collapse_state_syncs_after_manual_buffer_fold(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
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

    // Search for "ONE" which appears in all 3 files
    perform_search(search_view, "ONE", cx);

    // Verify initial state: no folds
    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(!has_any_folded, "No buffers should be folded initially");

    // Fold all via fold_all
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.fold_all(&FoldAll, window, cx);
            })
        })
        .expect("Should fold fine");
    cx.run_until_parked();

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "All buffers should be folded after fold_all"
    );

    // Manually unfold one buffer (simulating a chevron click)
    let first_buffer_id = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .buffer()
                .read(cx)
                .snapshot(cx)
                .excerpts()
                .next()
                .unwrap()
                .context
                .start
                .buffer_id
        })
        .expect("should read buffer ids");

    search_view
        .update(cx, |search_view, _window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.unfold_buffer(first_buffer_id, cx);
            })
        })
        .expect("Should unfold one buffer");

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "Should still report folds when only one buffer is unfolded"
    );

    // Unfold all via unfold_all
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.unfold_all(&UnfoldAll, window, cx);
            })
        })
        .expect("Should unfold fine");
    cx.run_until_parked();

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(!has_any_folded, "No folds should remain after unfold_all");

    // Manually fold one buffer back (simulating a chevron click)
    search_view
        .update(cx, |search_view, _window, cx| {
            search_view.results_editor.update(cx, |editor, cx| {
                editor.fold_buffer(first_buffer_id, cx);
            })
        })
        .expect("Should fold one buffer");

    let has_any_folded = search_view
        .read_with(cx, |search_view, cx| {
            search_view
                .results_editor
                .read(cx)
                .has_any_buffer_folded(cx)
        })
        .expect("should read state");
    assert!(
        has_any_folded,
        "Should report folds after manually folding one buffer"
    );
}

#[perf]
#[gpui::test]
async fn test_deploy_project_search_focus(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
            "four.rs": "const FOUR: usize = one::ONE + three::THREE;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    let active_item = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    });
    assert!(
        active_item.is_none(),
        "Expected no search panel to be active"
    );

    workspace.update_in(cx, move |workspace, window, cx| {
        assert_eq!(workspace.panes().len(), 1);
        workspace.panes()[0].update(cx, |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });

        ProjectSearchView::deploy_search(workspace, &workspace::DeploySearch::default(), window, cx)
    });

    let Some(search_view) = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    }) else {
        panic!("Search view expected to appear after new search event trigger")
    };

    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();
    window
            .update(cx, |_, window, cx| {
                search_view.update(cx, |search_view, cx| {
                    assert!(
                        search_view.query_editor.focus_handle(cx).is_focused(window),
                        "Empty search view should be focused after the toggle focus event: no results panel to focus on",
                    );
                });
        }).unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                let query_editor = &search_view.query_editor;
                assert!(
                    query_editor.focus_handle(cx).is_focused(window),
                    "Search view should be focused after the new search view is activated",
                );
                let query_text = query_editor.read(cx).text(cx);
                assert!(
                    query_text.is_empty(),
                    "New search query should be empty but got '{query_text}'",
                );
                let results_text = search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx));
                assert!(
                    results_text.is_empty(),
                    "Empty search view should have no results but got '{results_text}'"
                );
            });
        })
        .unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("sOMETHINGtHATsURELYdOESnOTeXIST", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
            .update(cx, |_, window, cx| {
                search_view.update(cx, |search_view, cx| {
                    let results_text = search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx));
                    assert!(
                        results_text.is_empty(),
                        "Search view for mismatching query should have no results but got '{results_text}'"
                    );
                    assert!(
                        search_view.query_editor.focus_handle(cx).is_focused(window),
                        "Search view should be focused after mismatching query had been used in search",
                    );
                });
            }).unwrap();

    cx.spawn(|mut cx| async move {
        window.update(&mut cx, |_, window, cx| {
            window.dispatch_action(ToggleFocus.boxed_clone(), cx)
        })
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert!(
                    search_view.query_editor.focus_handle(cx).is_focused(window),
                    "Search view with mismatching query should be focused after the toggle focus event: still no results panel to focus on",
                );
            });
        }).unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("TWO", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst THREE: usize = one::ONE + two::TWO;\n\n\nconst TWO: usize = one::ONE + one::ONE;",
                    "Search view results should match the query"
                );
                assert!(
                    search_view.results_editor.focus_handle(cx).is_focused(window),
                    "Search view with mismatching query should be focused after search results are available",
                );
            });
        }).unwrap();
    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert!(
                    search_view.results_editor.focus_handle(cx).is_focused(window),
                    "Search view with matching query should still have its results editor focused after the toggle focus event",
                );
            });
        }).unwrap();

    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(workspace, &workspace::DeploySearch::default(), window, cx)
    });
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "two", "Query should be updated to first search result after search view 2nd open in a row");
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst THREE: usize = one::ONE + two::TWO;\n\n\nconst TWO: usize = one::ONE + one::ONE;",
                    "Results should be unchanged after search view 2nd open in a row"
                );
                assert!(
                    search_view.query_editor.focus_handle(cx).is_focused(window),
                    "Focus should be moved into query editor again after search view 2nd open in a row"
                );
            });
        }).unwrap();

    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert!(
                    search_view.results_editor.focus_handle(cx).is_focused(window),
                    "Search view with matching query should switch focus to the results editor after the toggle focus event",
                );
            });
        }).unwrap();
}

#[perf]
#[gpui::test]
async fn test_filters_consider_toggle_state(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            "one.rs": "const ONE: usize = 1;",
            "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            "three.rs": "const THREE: usize = one::ONE + two::TWO;",
            "four.rs": "const FOUR: usize = one::ONE + three::THREE;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    workspace.update_in(cx, move |workspace, window, cx| {
        workspace.panes()[0].update(cx, |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });

        ProjectSearchView::deploy_search(workspace, &workspace::DeploySearch::default(), window, cx)
    });

    let Some(search_view) = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    }) else {
        panic!("Search view expected to appear after new search event trigger")
    };

    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("const FOUR", window, cx)
                });
                search_view.toggle_filters(cx);
                search_view
                    .excluded_files_editor
                    .update(cx, |exclude_editor, cx| {
                        exclude_editor.set_text("four.rs", window, cx)
                    });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
            .update(cx, |_, _, cx| {
                search_view.update(cx, |search_view, cx| {
                    let results_text = search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx));
                    assert!(
                        results_text.is_empty(),
                        "Search view for query with the only match in an excluded file should have no results but got '{results_text}'"
                    );
                });
            }).unwrap();

    cx.spawn(|mut cx| async move {
        window.update(&mut cx, |_, window, cx| {
            window.dispatch_action(ToggleFocus.boxed_clone(), cx)
        })
    })
    .detach();
    cx.background_executor.run_until_parked();

    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.toggle_filters(cx);
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
            .update(cx, |_, _, cx| {
                search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst FOUR: usize = one::ONE + three::THREE;",
                    "Search view results should contain the queried result in the previously excluded file with filters toggled off"
                );
            });
            })
            .unwrap();
}

#[perf]
#[gpui::test]
async fn test_new_project_search_focus(cx: &mut TestAppContext) {
    init_test(cx);

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
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    let active_item = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    });
    assert!(
        active_item.is_none(),
        "Expected no search panel to be active"
    );

    workspace.update_in(cx, move |workspace, window, cx| {
        assert_eq!(workspace.panes().len(), 1);
        workspace.panes()[0].update(cx, |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });

        ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
    });

    let Some(search_view) = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    }) else {
        panic!("Search view expected to appear after new search event trigger")
    };

    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();

    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                    assert!(
                        search_view.query_editor.focus_handle(cx).is_focused(window),
                        "Empty search view should be focused after the toggle focus event: no results panel to focus on",
                    );
                });
        }).unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                let query_editor = &search_view.query_editor;
                assert!(
                    query_editor.focus_handle(cx).is_focused(window),
                    "Search view should be focused after the new search view is activated",
                );
                let query_text = query_editor.read(cx).text(cx);
                assert!(
                    query_text.is_empty(),
                    "New search query should be empty but got '{query_text}'",
                );
                let results_text = search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx));
                assert!(
                    results_text.is_empty(),
                    "Empty search view should have no results but got '{results_text}'"
                );
            });
        })
        .unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("sOMETHINGtHATsURELYdOESnOTeXIST", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();

    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                    let results_text = search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx));
                    assert!(
                results_text.is_empty(),
                "Search view for mismatching query should have no results but got '{results_text}'"
            );
                    assert!(
                search_view.query_editor.focus_handle(cx).is_focused(window),
                "Search view should be focused after mismatching query had been used in search",
            );
                });
        })
        .unwrap();
    cx.spawn(|mut cx| async move {
        window.update(&mut cx, |_, window, cx| {
            window.dispatch_action(ToggleFocus.boxed_clone(), cx)
        })
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                    assert!(
                        search_view.query_editor.focus_handle(cx).is_focused(window),
                        "Search view with mismatching query should be focused after the toggle focus event: still no results panel to focus on",
                    );
                });
        }).unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("TWO", window, cx)
                });
                search_view.search(cx);
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx|
        search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst THREE: usize = one::ONE + two::TWO;\n\n\nconst TWO: usize = one::ONE + one::ONE;",
                    "Search view results should match the query"
                );
                assert!(
                    search_view.results_editor.focus_handle(cx).is_focused(window),
                    "Search view with mismatching query should be focused after search results are available",
                );
            })).unwrap();
    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                    assert!(
                        search_view.results_editor.focus_handle(cx).is_focused(window),
                        "Search view with matching query should still have its results editor focused after the toggle focus event",
                    );
                });
        }).unwrap();

    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
    });
    cx.background_executor.run_until_parked();
    let Some(search_view_2) = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    }) else {
        panic!("Search view expected to appear after new search event trigger")
    };
    assert!(
        search_view_2 != search_view,
        "New search view should be open after `workspace::NewSearch` event"
    );

    window.update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                    assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO", "First search view should not have an updated query");
                    assert_eq!(
                        search_view
                            .results_editor
                            .update(cx, |editor, cx| editor.display_text(cx)),
                        "\n\nconst THREE: usize = one::ONE + two::TWO;\n\n\nconst TWO: usize = one::ONE + one::ONE;",
                        "Results of the first search view should not update too"
                    );
                    assert!(
                        !search_view.query_editor.focus_handle(cx).is_focused(window),
                        "Focus should be moved away from the first search view"
                    );
                });
        }).unwrap();

    window.update(cx, |_, window, cx| {
            search_view_2.update(cx, |search_view_2, cx| {
                    assert_eq!(
                        search_view_2.query_editor.read(cx).text(cx),
                        "two",
                        "New search view should get the query from the text cursor was at during the event spawn (first search view's first result)"
                    );
                    assert_eq!(
                        search_view_2
                            .results_editor
                            .update(cx, |editor, cx| editor.display_text(cx)),
                        "",
                        "No search results should be in the 2nd view yet, as we did not spawn a search for it"
                    );
                    assert!(
                        search_view_2.query_editor.focus_handle(cx).is_focused(window),
                        "Focus should be moved into query editor of the new window"
                    );
                });
        }).unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view_2.update(cx, |search_view_2, cx| {
                search_view_2.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("FOUR", window, cx)
                });
                search_view_2.search(cx);
            });
        })
        .unwrap();

    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view_2.update(cx, |search_view_2, cx| {
                    assert_eq!(
                        search_view_2
                            .results_editor
                            .update(cx, |editor, cx| editor.display_text(cx)),
                        "\n\nconst FOUR: usize = one::ONE + three::THREE;",
                        "New search view with the updated query should have new search results"
                    );
                    assert!(
                        search_view_2.results_editor.focus_handle(cx).is_focused(window),
                        "Search view with mismatching query should be focused after search results are available",
                    );
                });
        }).unwrap();

    cx.spawn(|mut cx| async move {
        window
            .update(&mut cx, |_, window, cx| {
                window.dispatch_action(ToggleFocus.boxed_clone(), cx)
            })
            .unwrap();
    })
    .detach();
    cx.background_executor.run_until_parked();
    window.update(cx, |_, window, cx| {
            search_view_2.update(cx, |search_view_2, cx| {
                    assert!(
                        search_view_2.results_editor.focus_handle(cx).is_focused(window),
                        "Search view with matching query should switch focus to the results editor after the toggle focus event",
                    );
                });}).unwrap();
}

#[perf]
#[gpui::test]
async fn test_new_project_search_in_directory(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a": {
                "one.rs": "const ONE: usize = 1;",
                "two.rs": "const TWO: usize = one::ONE + one::ONE;",
            },
            "b": {
                "three.rs": "const THREE: usize = one::ONE + two::TWO;",
                "four.rs": "const FOUR: usize = one::ONE + three::THREE;",
            },
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    let active_item = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    });
    assert!(
        active_item.is_none(),
        "Expected no search panel to be active"
    );

    workspace.update_in(cx, move |workspace, window, cx| {
        assert_eq!(workspace.panes().len(), 1);
        workspace.panes()[0].update(cx, move |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });
    });

    let a_dir_entry = cx.update(|_, cx| {
        workspace
            .read(cx)
            .project()
            .read(cx)
            .entry_for_path(&(worktree_id, rel_path("a")).into(), cx)
            .expect("no entry for /a/ directory")
            .clone()
    });
    assert!(a_dir_entry.is_dir());
    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::new_search_in_directory(workspace, &a_dir_entry.path, window, cx)
    });

    let Some(search_view) = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
    }) else {
        panic!("Search view expected to appear after new search in directory event trigger")
    };
    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                assert!(
                    search_view.query_editor.focus_handle(cx).is_focused(window),
                    "On new search in directory, focus should be moved into query editor"
                );
                search_view.excluded_files_editor.update(cx, |editor, cx| {
                    assert!(
                        editor.display_text(cx).is_empty(),
                        "New search in directory should not have any excluded files"
                    );
                });
                search_view.included_files_editor.update(cx, |editor, cx| {
                    assert_eq!(
                        editor.display_text(cx),
                        a_dir_entry.path.display(PathStyle::local()),
                        "New search in directory should have included dir entry path"
                    );
                });
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("const", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view
                        .results_editor
                        .update(cx, |editor, cx| editor.display_text(cx)),
                    "\n\nconst ONE: usize = 1;\n\n\nconst TWO: usize = one::ONE + one::ONE;",
                    "New search in directory should have a filter that matches a certain directory"
                );
            })
        })
        .unwrap();
}

#[perf]
#[gpui::test]
async fn test_search_query_history(cx: &mut TestAppContext) {
    init_test(cx);

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
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    workspace.update_in(cx, {
        let search_bar = search_bar.clone();
        |workspace, window, cx| {
            assert_eq!(workspace.panes().len(), 1);
            workspace.panes()[0].update(cx, |pane, cx| {
                pane.toolbar()
                    .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
            });

            ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
        }
    });

    let search_view = cx.read(|cx| {
        workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>())
            .expect("Search view expected to appear after new search event trigger")
    });

    // Add 3 search items into the history + another unsubmitted one.
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.search_options = SearchOptions::CASE_SENSITIVE;
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("ONE", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();

    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("TWO", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("THREE", window, cx)
                });
                search_view.search(cx);
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("JUST_TEXT_INPUT", window, cx)
                });
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();

    // Ensure that the latest input with search settings is active.
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view.query_editor.read(cx).text(cx),
                    "JUST_TEXT_INPUT"
                );
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Next history query after the latest should preserve the current query.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            })
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view.query_editor.read(cx).text(cx),
                    "JUST_TEXT_INPUT"
                );
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            })
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(
                    search_view.query_editor.read(cx).text(cx),
                    "JUST_TEXT_INPUT"
                );
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Previous query should navigate backwards through history.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Further previous items should go over the history in reverse order.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "ONE");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Previous items should never go behind the first history item.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "ONE");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "ONE");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Next items should go over the history in the original order.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("TWO_NEW", window, cx)
                });
                search_view.search(cx);
            });
        })
        .unwrap();
    cx.background_executor.run_until_parked();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO_NEW");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // New search input should add another entry to history and move the selection to the end of the history.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "THREE");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "THREE");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO_NEW");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO_NEW");
                assert_eq!(search_view.search_options, SearchOptions::CASE_SENSITIVE);
            });
        })
        .unwrap();

    // Typing text without running a search, then navigating history, should allow
    // restoring the draft when pressing next past the end.
    window
        .update(cx, |_, window, cx| {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.update(cx, |query_editor, cx| {
                    query_editor.set_text("unsaved draft", window, cx)
                });
            })
        })
        .unwrap();
    cx.background_executor.run_until_parked();

    // Navigate up into history — the draft should be stashed.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "THREE");
            });
        })
        .unwrap();

    // Navigate forward through history.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "TWO_NEW");
            });
        })
        .unwrap();

    // Navigate past the end — the draft should be restored.
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.focus_search(window, cx);
                search_bar.next_history_query(&NextHistoryQuery, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            search_view.update(cx, |search_view, cx| {
                assert_eq!(search_view.query_editor.read(cx).text(cx), "unsaved draft");
            });
        })
        .unwrap();
}

#[perf]
#[gpui::test]
async fn test_search_query_history_with_multiple_views(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree_id = project.update(cx, |this, cx| {
        this.worktrees(cx).next().unwrap().read(cx).id()
    });

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panes: Vec<_> = workspace.update_in(cx, |this, _, _| this.panes().to_owned());

    let search_bar_1 = window.build_entity(cx, |_, _| ProjectSearchBar::new());
    let search_bar_2 = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    assert_eq!(panes.len(), 1);
    let first_pane = panes.first().cloned().unwrap();
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 0);
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("one.rs")),
                Some(first_pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 1);

    // Add a project search item to the first pane
    workspace.update_in(cx, {
        let search_bar = search_bar_1.clone();
        |workspace, window, cx| {
            first_pane.update(cx, |pane, cx| {
                pane.toolbar()
                    .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
            });

            ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
        }
    });
    let search_view_1 = cx.read(|cx| {
        workspace
            .read(cx)
            .active_item(cx)
            .and_then(|item| item.downcast::<ProjectSearchView>())
            .expect("Search view expected to appear after new search event trigger")
    });

    let second_pane = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.split_and_clone(
                first_pane.clone(),
                workspace::SplitDirection::Right,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(cx.update(|_, cx| second_pane.read(cx).items_len()), 1);

    assert_eq!(cx.update(|_, cx| second_pane.read(cx).items_len()), 1);
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 2);

    // Add a project search item to the second pane
    workspace.update_in(cx, {
        let search_bar = search_bar_2.clone();
        let pane = second_pane.clone();
        move |workspace, window, cx| {
            assert_eq!(workspace.panes().len(), 2);
            pane.update(cx, |pane, cx| {
                pane.toolbar()
                    .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
            });

            ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
        }
    });

    let search_view_2 = cx.read(|cx| {
        workspace
            .read(cx)
            .active_item(cx)
            .and_then(|item| item.downcast::<ProjectSearchView>())
            .expect("Search view expected to appear after new search event trigger")
    });

    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 2);
    assert_eq!(cx.update(|_, cx| second_pane.read(cx).items_len()), 2);

    let update_search_view =
        |search_view: &Entity<ProjectSearchView>, query: &str, cx: &mut TestAppContext| {
            window
                .update(cx, |_, window, cx| {
                    search_view.update(cx, |search_view, cx| {
                        search_view.query_editor.update(cx, |query_editor, cx| {
                            query_editor.set_text(query, window, cx)
                        });
                        search_view.search(cx);
                    });
                })
                .unwrap();
        };

    let active_query =
        |search_view: &Entity<ProjectSearchView>, cx: &mut TestAppContext| -> String {
            window
                .update(cx, |_, _, cx| {
                    search_view.update(cx, |search_view, cx| {
                        search_view.query_editor.read(cx).text(cx)
                    })
                })
                .unwrap()
        };

    let select_prev_history_item = |search_bar: &Entity<ProjectSearchBar>,
                                    cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, cx| {
                search_bar.update(cx, |search_bar, cx| {
                    search_bar.focus_search(window, cx);
                    search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
                })
            })
            .unwrap();
    };

    let select_next_history_item = |search_bar: &Entity<ProjectSearchBar>,
                                    cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, cx| {
                search_bar.update(cx, |search_bar, cx| {
                    search_bar.focus_search(window, cx);
                    search_bar.next_history_query(&NextHistoryQuery, window, cx);
                })
            })
            .unwrap();
    };

    update_search_view(&search_view_1, "ONE", cx);
    cx.background_executor.run_until_parked();

    update_search_view(&search_view_2, "TWO", cx);
    cx.background_executor.run_until_parked();

    assert_eq!(active_query(&search_view_1, cx), "ONE");
    assert_eq!(active_query(&search_view_2, cx), "TWO");

    // Selecting previous history item should select the query from search view 1.
    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "ONE");

    // Selecting the previous history item should not change the query as it is already the first item.
    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "ONE");

    // Changing the query in search view 2 should not affect the history of search view 1.
    assert_eq!(active_query(&search_view_1, cx), "ONE");

    // Deploying a new search in search view 2
    update_search_view(&search_view_2, "THREE", cx);
    cx.background_executor.run_until_parked();

    select_next_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "THREE");

    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "TWO");

    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "ONE");

    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "ONE");

    select_prev_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "ONE");

    // Search view 1 should now see the query from search view 2.
    assert_eq!(active_query(&search_view_1, cx), "ONE");

    select_next_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "TWO");

    // Here is the new query from search view 2
    select_next_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "THREE");

    select_next_history_item(&search_bar_2, cx);
    assert_eq!(active_query(&search_view_2, cx), "THREE");

    select_next_history_item(&search_bar_1, cx);
    assert_eq!(active_query(&search_view_1, cx), "TWO");

    select_next_history_item(&search_bar_1, cx);
    assert_eq!(active_query(&search_view_1, cx), "THREE");

    select_next_history_item(&search_bar_1, cx);
    assert_eq!(active_query(&search_view_1, cx), "THREE");
}

#[perf]
#[gpui::test]
async fn test_deploy_search_with_multiple_panes(cx: &mut TestAppContext) {
    init_test(cx);

    // Setup 2 panes, both with a file open and one with a project search.
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree_id = project.update(cx, |this, cx| {
        this.worktrees(cx).next().unwrap().read(cx).id()
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panes: Vec<_> = workspace.update_in(cx, |this, _, _| this.panes().to_owned());
    assert_eq!(panes.len(), 1);
    let first_pane = panes.first().cloned().unwrap();
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 0);
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("one.rs")),
                Some(first_pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 1);
    let second_pane = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.split_and_clone(
                first_pane.clone(),
                workspace::SplitDirection::Right,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(cx.update(|_, cx| second_pane.read(cx).items_len()), 1);
    assert!(
        window
            .update(cx, |_, window, cx| second_pane
                .focus_handle(cx)
                .contains_focused(window, cx))
            .unwrap()
    );
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());
    workspace.update_in(cx, {
        let search_bar = search_bar.clone();
        let pane = first_pane.clone();
        move |workspace, window, cx| {
            assert_eq!(workspace.panes().len(), 2);
            pane.update(cx, move |pane, cx| {
                pane.toolbar()
                    .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
            });
        }
    });

    // Add a project search item to the second pane
    workspace.update_in(cx, {
        |workspace, window, cx| {
            assert_eq!(workspace.panes().len(), 2);
            second_pane.update(cx, |pane, cx| {
                pane.toolbar()
                    .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
            });

            ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
        }
    });

    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| second_pane.read(cx).items_len()), 2);
    assert_eq!(cx.update(|_, cx| first_pane.read(cx).items_len()), 1);

    // Focus the first pane
    workspace.update_in(cx, |workspace, window, cx| {
        assert_eq!(workspace.active_pane(), &second_pane);
        second_pane.update(cx, |this, cx| {
            assert_eq!(this.active_item_index(), 1);
            this.activate_previous_item(&Default::default(), window, cx);
            assert_eq!(this.active_item_index(), 0);
        });
        workspace.activate_pane_in_direction(workspace::SplitDirection::Left, window, cx);
    });
    workspace.update_in(cx, |workspace, _, cx| {
        assert_eq!(workspace.active_pane(), &first_pane);
        assert_eq!(first_pane.read(cx).items_len(), 1);
        assert_eq!(second_pane.read(cx).items_len(), 2);
    });

    // Deploy a new search
    cx.dispatch_action(DeploySearch::default());

    // Both panes should now have a project search in them
    workspace.update_in(cx, |workspace, window, cx| {
        assert_eq!(workspace.active_pane(), &first_pane);
        first_pane.read_with(cx, |this, _| {
            assert_eq!(this.active_item_index(), 1);
            assert_eq!(this.items_len(), 2);
        });
        second_pane.update(cx, |this, cx| {
            assert!(!cx.focus_handle().contains_focused(window, cx));
            assert_eq!(this.items_len(), 2);
        });
    });

    // Focus the second pane's non-search item
    window
        .update(cx, |_workspace, window, cx| {
            second_pane.update(cx, |pane, cx| {
                pane.activate_next_item(&Default::default(), window, cx)
            });
        })
        .unwrap();

    // Deploy a new search
    cx.dispatch_action(DeploySearch::default());

    // The project search view should now be focused in the second pane
    // And the number of items should be unchanged.
    window
        .update(cx, |_workspace, _, cx| {
            second_pane.update(cx, |pane, _cx| {
                assert!(
                    pane.active_item()
                        .unwrap()
                        .downcast::<ProjectSearchView>()
                        .is_some()
                );

                assert_eq!(pane.items_len(), 2);
            });
        })
        .unwrap();
}

#[perf]
#[gpui::test]
async fn test_scroll_search_results_to_top(cx: &mut TestAppContext) {
    init_test(cx);

    // We need many lines in the search results to be able to scroll the window
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "1.txt": "\n\n\n\n\n A \n\n\n\n\n",
            "2.txt": "\n\n\n\n\n A \n\n\n\n\n",
            "3.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "4.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "5.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "6.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "7.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "8.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "9.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "a.rs": "\n\n\n\n\n A \n\n\n\n\n",
            "b.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "c.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "d.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "e.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "f.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "g.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "h.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "i.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "j.rs": "\n\n\n\n\n B \n\n\n\n\n",
            "k.rs": "\n\n\n\n\n B \n\n\n\n\n",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project, cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    // First search
    perform_search(search_view, "A", cx);
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.results_editor.update(cx, |results_editor, cx| {
                // Results are correct and scrolled to the top
                assert_eq!(
                    results_editor.display_text(cx).match_indices(" A ").count(),
                    10
                );
                assert_eq!(results_editor.scroll_position(cx), Point::default());

                // Scroll results all the way down
                results_editor.scroll(Point::new(0., f64::MAX), Some(Axis::Vertical), window, cx);
            });
        })
        .expect("unable to update search view");

    // Second search
    perform_search(search_view, "B", cx);
    search_view
        .update(cx, |search_view, _, cx| {
            search_view.results_editor.update(cx, |results_editor, cx| {
                // Results are correct...
                assert_eq!(
                    results_editor.display_text(cx).match_indices(" B ").count(),
                    10
                );
                // ...and scrolled back to the top
                assert_eq!(results_editor.scroll_position(cx), Point::default());
            });
        })
        .expect("unable to update search view");
}

#[perf]
#[gpui::test]
async fn test_buffer_search_query_reused(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree_id = project.update(cx, |this, cx| {
        this.worktrees(cx).next().unwrap().read(cx).id()
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let editor = workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("one.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Wait for the unstaged changes to be loaded
    cx.run_until_parked();

    let buffer_search_bar = cx.new_window_entity(|window, cx| {
        let mut search_bar =
            BufferSearchBar::new(Some(project.read(cx).languages().clone()), window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    let panes: Vec<_> = workspace.update_in(&mut cx, |this, _, _| this.panes().to_owned());
    assert_eq!(panes.len(), 1);
    let pane = panes.first().cloned().unwrap();
    pane.update_in(&mut cx, |pane, window, cx| {
        pane.toolbar().update(cx, |toolbar, cx| {
            toolbar.add_item(buffer_search_bar.clone(), window, cx);
        })
    });

    let buffer_search_query = "search bar query";
    buffer_search_bar
        .update_in(&mut cx, |buffer_search_bar, window, cx| {
            buffer_search_bar.focus_handle(cx).focus(window, cx);
            buffer_search_bar.search(buffer_search_query, None, true, window, cx)
        })
        .await
        .unwrap();

    workspace.update_in(&mut cx, |workspace, window, cx| {
        ProjectSearchView::new_search(workspace, &workspace::NewSearch, window, cx)
    });
    cx.run_until_parked();
    let project_search_view = pane
        .read_with(&cx, |pane, _| {
            pane.active_item()
                .and_then(|item| item.downcast::<ProjectSearchView>())
        })
        .expect("should open a project search view after spawning a new search");
    project_search_view.update(&mut cx, |search_view, cx| {
            assert_eq!(
                search_view.search_query_text(cx),
                buffer_search_query,
                "Project search should take the query from the buffer search bar since it got focused and had a query inside"
            );
        });
}

#[gpui::test]
async fn test_search_dismisses_modal(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    struct EmptyModalView {
        focus_handle: gpui::FocusHandle,
    }
    impl EventEmitter<gpui::DismissEvent> for EmptyModalView {}
    impl Render for EmptyModalView {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            div()
        }
    }
    impl Focusable for EmptyModalView {
        fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
            self.focus_handle.clone()
        }
    }
    impl workspace::ModalView for EmptyModalView {}

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView {
            focus_handle: cx.focus_handle(),
        });
        assert!(workspace.has_active_modal(window, cx));
    });

    cx.dispatch_action(Deploy::find());

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.has_active_modal(window, cx));
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView {
            focus_handle: cx.focus_handle(),
        });
        assert!(workspace.has_active_modal(window, cx));
    });

    cx.dispatch_action(DeploySearch::default());

    workspace.update_in(cx, |workspace, window, cx| {
        assert!(!workspace.has_active_modal(window, cx));
    });
}

#[perf]
#[gpui::test]
async fn test_search_with_inlays(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    })
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        // `\n` , a trailing line on the end, is important for the test case
        json!({
            "main.rs": "fn main() { let a = 2; }\n",
        }),
    )
    .await;

    let requests_count = Arc::new(AtomicUsize::new(0));
    let closure_requests_count = requests_count.clone();
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language);
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let requests_count = closure_requests_count.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>({
                    move |_, _| {
                        let requests_count = requests_count.clone();
                        async move {
                            requests_count.fetch_add(1, atomic::Ordering::Release);
                            Ok(Some(vec![lsp::InlayHint {
                                position: lsp::Position::new(0, 17),
                                label: lsp::InlayHintLabel::String(": i32".to_owned()),
                                kind: Some(lsp::InlayHintKind::TYPE),
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            }]))
                        }
                    }
                });
            })),
            ..FakeLspAdapter::default()
        },
    );

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    perform_search(search_view, "let ", cx);
    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        1,
        "New hints should have been queried",
    );

    // Can do the 2nd search without any panics
    perform_search(search_view, "let ", cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        2,
        "We did drop the previous buffer when cleared the old project search results, hence another query was made",
    );

    let singleton_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/dir/main.rs")),
                workspace::OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "fn main() { let a: i32 = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        2,
        "Opening the same buffer again should reuse the cached hints",
    );

    window
        .update(cx, |_, window, cx| {
            singleton_editor.update(cx, |editor, cx| {
                editor.handle_input("test", window, cx);
            });
        })
        .unwrap();

    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "testfn main() { l: i32et a = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        3,
        "We have edited the buffer and should send a new request",
    );

    window
        .update(cx, |_, window, cx| {
            singleton_editor.update(cx, |editor, cx| {
                editor.undo(&editor::actions::Undo, window, cx);
            });
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        4,
        "We have edited the buffer again and should send a new request again",
    );
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "fn main() { let a: i32 = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    project.update(cx, |_, cx| {
        cx.emit(project::Event::RefreshInlayHints {
            server_id: fake_server.server.server_id(),
            request_id: Some(1),
        });
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        5,
        "After a simulated server refresh request, we should have sent another request",
    );

    perform_search(search_view, "let ", cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        5,
        "New project search should reuse the cached hints",
    );
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_deleted_file_removed_from_search_results(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a.txt": "hello world",
            "file_b.txt": "hello universe",
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

    perform_search(search_view, "hello", cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            let match_count = search_view.entity.read(cx).match_ranges.len();
            assert_eq!(match_count, 2, "Should have matches from both files");
        })
        .unwrap();

    // Delete file_b.txt
    fs.remove_file(
        path!("/dir/file_b.txt").as_ref(),
        fs::RemoveOptions::default(),
    )
    .await
    .unwrap();
    cx.run_until_parked();

    // Verify deleted file's results are removed proactively
    search_view
        .update(cx, |search_view, _window, cx| {
            let results_text = search_view
                .results_editor
                .update(cx, |editor, cx| editor.display_text(cx));
            assert!(
                !results_text.contains("universe"),
                "Deleted file's content should be removed from results, got: {results_text}"
            );
            assert!(
                results_text.contains("world"),
                "Remaining file's content should still be present, got: {results_text}"
            );
        })
        .unwrap();

    // Re-run the search and verify deleted file stays gone
    perform_search(search_view, "hello", cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            let results_text = search_view
                .results_editor
                .update(cx, |editor, cx| editor.display_text(cx));
            assert!(
                !results_text.contains("universe"),
                "Deleted file should not reappear after re-search, got: {results_text}"
            );
            assert!(
                results_text.contains("world"),
                "Remaining file should still be found, got: {results_text}"
            );
            assert_eq!(
                search_view.entity.read(cx).match_ranges.len(),
                1,
                "Should only have match from the remaining file"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_deploy_search_applies_and_resets_options(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.panes()[0].update(cx, |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });

        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                regex: Some(true),
                case_sensitive: Some(true),
                whole_word: Some(true),
                include_ignored: Some(true),
                query: Some("Test_Query".into()),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    let search_view = cx
        .read(|cx| {
            workspace
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .and_then(|item| item.downcast::<ProjectSearchView>())
        })
        .expect("Search view should be active after deploy");

    search_view.update_in(cx, |search_view, _window, cx| {
        assert!(
            search_view.search_options.contains(SearchOptions::REGEX),
            "Regex option should be enabled"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::CASE_SENSITIVE),
            "Case sensitive option should be enabled"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::WHOLE_WORD),
            "Whole word option should be enabled"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::INCLUDE_IGNORED),
            "Include ignored option should be enabled"
        );
        let query_text = search_view.query_editor.read(cx).text(cx);
        assert_eq!(
            query_text, "Test_Query",
            "Query should be set from the action"
        );
    });

    // Redeploy with only regex - unspecified options should be preserved.
    cx.dispatch_action(menu::Cancel);
    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                regex: Some(true),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    search_view.update_in(cx, |search_view, _window, _cx| {
        assert!(
            search_view.search_options.contains(SearchOptions::REGEX),
            "Regex should still be enabled"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::CASE_SENSITIVE),
            "Case sensitive should be preserved from previous deploy"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::WHOLE_WORD),
            "Whole word should be preserved from previous deploy"
        );
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::INCLUDE_IGNORED),
            "Include ignored should be preserved from previous deploy"
        );
    });

    // Redeploy explicitly turning off options.
    cx.dispatch_action(menu::Cancel);
    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                regex: Some(true),
                case_sensitive: Some(false),
                whole_word: Some(false),
                include_ignored: Some(false),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    search_view.update_in(cx, |search_view, _window, _cx| {
        assert_eq!(
            search_view.search_options,
            SearchOptions::REGEX,
            "Explicit Some(false) should turn off options"
        );
    });

    // Redeploy with an empty query - should not overwrite the existing query.
    cx.dispatch_action(menu::Cancel);
    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                query: Some("".into()),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    search_view.update_in(cx, |search_view, _window, cx| {
        let query_text = search_view.query_editor.read(cx).text(cx);
        assert_eq!(
            query_text, "Test_Query",
            "Empty query string should not overwrite the existing query"
        );
    });
}

#[gpui::test]
async fn test_replace_all_with_shared_heading_prefix_does_not_loop(cx: &mut TestAppContext) {
    init_test(cx);

    let search_text = "## この日に作成したノート";
    let replacement_text = "## この日に関連するノート";

    let file_a_before = format!("{search_text}\n- a\n\n{search_text}\n- b\n");
    let file_b_before = format!("# Daily\n\n{search_text}\n- c\n");
    let file_a_after = format!("{replacement_text}\n- a\n\n{replacement_text}\n- b\n");
    let file_b_after = format!("# Daily\n\n{replacement_text}\n- c\n");

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.md": file_a_before,
            "b.md": file_b_before,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    perform_search(search_view, search_text, cx);

    search_view
        .update(cx, |search_view, _window, cx| {
            assert_eq!(search_view.entity.read(cx).match_ranges.len(), 3);
        })
        .unwrap();

    search_view
        .update(cx, |search_view, window, cx| {
            search_view.replacement_editor.update(cx, |editor, cx| {
                editor.set_text(replacement_text, window, cx);
            });
            search_view.replace_all(&ReplaceAll, window, cx);
        })
        .unwrap();

    cx.run_until_parked();

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("a.md")), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("b.md")), cx)
        })
        .await
        .unwrap();

    assert_eq!(
        buffer_a.read_with(cx, |buffer, _| buffer.text()),
        file_a_after
    );
    assert_eq!(
        buffer_b.read_with(cx, |buffer, _| buffer.text()),
        file_b_after
    );
}

#[gpui::test]
async fn test_smartcase_overrides_explicit_case_sensitive(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_default_settings(cx, |settings| {
                settings.editor.use_smartcase_search = Some(true);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "one.rs": "const ONE: usize = 1;",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search_bar = window.build_entity(cx, |_, _| ProjectSearchBar::new());

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.panes()[0].update(cx, |pane, cx| {
            pane.toolbar()
                .update(cx, |toolbar, cx| toolbar.add_item(search_bar, window, cx))
        });

        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                case_sensitive: Some(true),
                query: Some("lowercase_query".into()),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    let search_view = cx
        .read(|cx| {
            workspace
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .and_then(|item| item.downcast::<ProjectSearchView>())
        })
        .expect("Search view should be active after deploy");

    // Smartcase should override the explicit case_sensitive flag
    // because the query is all lowercase.
    search_view.update_in(cx, |search_view, _window, cx| {
        assert!(
            !search_view
                .search_options
                .contains(SearchOptions::CASE_SENSITIVE),
            "Smartcase should disable case sensitivity for a lowercase query, \
                 even when case_sensitive was explicitly set in the action"
        );
        let query_text = search_view.query_editor.read(cx).text(cx);
        assert_eq!(query_text, "lowercase_query");
    });

    // Now deploy with an uppercase query - smartcase should enable case sensitivity.
    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(
            workspace,
            &workspace::DeploySearch {
                query: Some("Uppercase_Query".into()),
                ..Default::default()
            },
            window,
            cx,
        )
    });

    search_view.update_in(cx, |search_view, _window, cx| {
        assert!(
            search_view
                .search_options
                .contains(SearchOptions::CASE_SENSITIVE),
            "Smartcase should enable case sensitivity for a query containing uppercase"
        );
        let query_text = search_view.query_editor.read(cx).text(cx);
        assert_eq!(query_text, "Uppercase_Query");
    });
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);

        theme_settings::init(theme::LoadThemes::JustBase, cx);

        editor::init(cx);
        crate::init(cx);
    });
}

fn perform_search(
    search_view: WindowHandle<ProjectSearchView>,
    text: impl Into<Arc<str>>,
    cx: &mut TestAppContext,
) {
    search_view
        .update(cx, |search_view, window, cx| {
            search_view.query_editor.update(cx, |query_editor, cx| {
                query_editor.set_text(text, window, cx)
            });
            search_view.search(cx);
        })
        .unwrap();
    // Ensure editor highlights appear after the search is done
    cx.executor()
        .advance_clock(editor::SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT + Duration::from_millis(100));
    cx.background_executor.run_until_parked();
}
