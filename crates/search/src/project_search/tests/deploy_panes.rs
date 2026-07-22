use super::*;

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
