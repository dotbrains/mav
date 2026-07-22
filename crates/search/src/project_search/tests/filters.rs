use super::*;

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
