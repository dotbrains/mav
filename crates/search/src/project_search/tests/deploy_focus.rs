use super::*;

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
