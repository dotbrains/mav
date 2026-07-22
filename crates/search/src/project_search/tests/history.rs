use super::*;

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
