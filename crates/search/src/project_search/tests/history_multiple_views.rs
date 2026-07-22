use super::*;

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
