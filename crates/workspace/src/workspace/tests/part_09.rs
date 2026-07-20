use super::*;

#[gpui::test]
async fn test_close_item_in_all_panes(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({ "test.txt": "" })).await;

    let project = Project::test(fs, ["root".as_ref()], cx).await;
    let (workspace, cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
    // Add item to pane A with project path
    let item_a = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "test.txt", cx)]));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(item_a.clone()), None, true, window, cx)
    });

    // Split to create pane B
    let pane_b = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
    });

    // Add item with SAME project path to pane B, and pin it
    let item_b = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "test.txt", cx)]));
    pane_b.update_in(cx, |pane, window, cx| {
        pane.add_item(Box::new(item_b.clone()), true, true, None, window, cx);
        pane.set_pinned_count(1);
    });

    assert_eq!(pane_a.read_with(cx, |pane, _| pane.items_len()), 1);
    assert_eq!(pane_b.read_with(cx, |pane, _| pane.items_len()), 1);

    // close_pinned: false should only close the unpinned copy
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.close_item_in_all_panes(
            &CloseItemInAllPanes {
                save_intent: Some(SaveIntent::Close),
                close_pinned: false,
            },
            window,
            cx,
        )
    });
    cx.executor().run_until_parked();

    let item_count_a = pane_a.read_with(cx, |pane, _| pane.items_len());
    let item_count_b = pane_b.read_with(cx, |pane, _| pane.items_len());
    assert_eq!(item_count_a, 0, "Unpinned item in pane A should be closed");
    assert_eq!(item_count_b, 1, "Pinned item in pane B should remain");

    // Split again, seeing as closing the previous item also closed its
    // pane, so only pane remains, which does not allow us to properly test
    // that both items close when `close_pinned: true`.
    let pane_c = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_b.clone(), SplitDirection::Right, window, cx)
    });

    // Add an item with the same project path to pane C so that
    // close_item_in_all_panes can determine what to close across all panes
    // (it reads the active item from the active pane, and split_pane
    // creates an empty pane).
    let item_c = cx
        .new(|cx| TestItem::new(cx).with_project_items(&[TestProjectItem::new(1, "test.txt", cx)]));
    pane_c.update_in(cx, |pane, window, cx| {
        pane.add_item(Box::new(item_c.clone()), true, true, None, window, cx);
    });

    // close_pinned: true should close the pinned copy too
    workspace.update_in(cx, |workspace, window, cx| {
        let panes_count = workspace.panes().len();
        assert_eq!(panes_count, 2, "Workspace should have two panes (B and C)");

        workspace.close_item_in_all_panes(
            &CloseItemInAllPanes {
                save_intent: Some(SaveIntent::Close),
                close_pinned: true,
            },
            window,
            cx,
        )
    });
    cx.executor().run_until_parked();

    let item_count_b = pane_b.read_with(cx, |pane, _| pane.items_len());
    let item_count_c = pane_c.read_with(cx, |pane, _| pane.items_len());
    assert_eq!(item_count_b, 0, "Pinned item in pane B should be closed");
    assert_eq!(item_count_c, 0, "Unpinned item in pane C should be closed");
}

mod register_project_item_tests {

    use super::*;

    // View
    struct TestPngItemView {
        focus_handle: FocusHandle,
    }
    // Model
    struct TestPngItem {}

    impl project::ProjectItem for TestPngItem {
        fn try_open(
            _project: &Entity<Project>,
            path: &ProjectPath,
            cx: &mut App,
        ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
            if path.path.extension().unwrap() == "png" {
                Some(cx.spawn(async move |cx| Ok(cx.new(|_| TestPngItem {}))))
            } else {
                None
            }
        }

        fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
            None
        }

        fn project_path(&self, _: &App) -> Option<ProjectPath> {
            None
        }

        fn is_dirty(&self) -> bool {
            false
        }
    }

    impl Item for TestPngItemView {
        type Event = ();
        fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
            "".into()
        }
    }
    impl EventEmitter<()> for TestPngItemView {}
    impl Focusable for TestPngItemView {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for TestPngItemView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl ProjectItem for TestPngItemView {
        type Item = TestPngItem;

        fn for_project_item(
            _project: Entity<Project>,
            _pane: Option<&Pane>,
            _item: Entity<Self::Item>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Self
        where
            Self: Sized,
        {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    // View
    struct TestIpynbItemView {
        focus_handle: FocusHandle,
    }
    // Model
    struct TestIpynbItem {}

    impl project::ProjectItem for TestIpynbItem {
        fn try_open(
            _project: &Entity<Project>,
            path: &ProjectPath,
            cx: &mut App,
        ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
            if path.path.extension().unwrap() == "ipynb" {
                Some(cx.spawn(async move |cx| Ok(cx.new(|_| TestIpynbItem {}))))
            } else {
                None
            }
        }

        fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
            None
        }

        fn project_path(&self, _: &App) -> Option<ProjectPath> {
            None
        }

        fn is_dirty(&self) -> bool {
            false
        }
    }

    impl Item for TestIpynbItemView {
        type Event = ();
        fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
            "".into()
        }
    }
    impl EventEmitter<()> for TestIpynbItemView {}
    impl Focusable for TestIpynbItemView {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for TestIpynbItemView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl ProjectItem for TestIpynbItemView {
        type Item = TestIpynbItem;

        fn for_project_item(
            _project: Entity<Project>,
            _pane: Option<&Pane>,
            _item: Entity<Self::Item>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Self
        where
            Self: Sized,
        {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    struct TestAlternatePngItemView {
        focus_handle: FocusHandle,
    }

    impl Item for TestAlternatePngItemView {
        type Event = ();
        fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
            "".into()
        }
    }

    impl EventEmitter<()> for TestAlternatePngItemView {}
    impl Focusable for TestAlternatePngItemView {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for TestAlternatePngItemView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl ProjectItem for TestAlternatePngItemView {
        type Item = TestPngItem;

        fn for_project_item(
            _project: Entity<Project>,
            _pane: Option<&Pane>,
            _item: Entity<Self::Item>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) -> Self
        where
            Self: Sized,
        {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    #[gpui::test]
    async fn test_register_project_item(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            register_project_item::<TestPngItemView>(cx);
            register_project_item::<TestIpynbItemView>(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root1",
            json!({
                "one.png": "BINARYDATAHERE",
                "two.ipynb": "{ totally a notebook }",
                "three.txt": "editing text, sure why not?"
            }),
        )
        .await;

        let project = Project::test(fs, ["root1".as_ref()], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

        let worktree_id = project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let handle = workspace
            .update_in(cx, |workspace, window, cx| {
                let project_path = (worktree_id, rel_path("one.png"));
                workspace.open_path(project_path, None, true, window, cx)
            })
            .await
            .unwrap();

        // Now we can check if the handle we got back errored or not
        assert_eq!(
            handle.to_any_view().entity_type(),
            TypeId::of::<TestPngItemView>()
        );

        let handle = workspace
            .update_in(cx, |workspace, window, cx| {
                let project_path = (worktree_id, rel_path("two.ipynb"));
                workspace.open_path(project_path, None, true, window, cx)
            })
            .await
            .unwrap();

        assert_eq!(
            handle.to_any_view().entity_type(),
            TypeId::of::<TestIpynbItemView>()
        );

        let handle = workspace
            .update_in(cx, |workspace, window, cx| {
                let project_path = (worktree_id, rel_path("three.txt"));
                workspace.open_path(project_path, None, true, window, cx)
            })
            .await;
        assert!(handle.is_err());
    }

    #[gpui::test]
    async fn test_register_project_item_two_enter_one_leaves(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            register_project_item::<TestPngItemView>(cx);
            register_project_item::<TestAlternatePngItemView>(cx);
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/root1",
            json!({
                "one.png": "BINARYDATAHERE",
                "two.ipynb": "{ totally a notebook }",
                "three.txt": "editing text, sure why not?"
            }),
        )
        .await;
        let project = Project::test(fs, ["root1".as_ref()], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let worktree_id = project.update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        let handle = workspace
            .update_in(cx, |workspace, window, cx| {
                let project_path = (worktree_id, rel_path("one.png"));
                workspace.open_path(project_path, None, true, window, cx)
            })
            .await
            .unwrap();

        // This _must_ be the second item registered
        assert_eq!(
            handle.to_any_view().entity_type(),
            TypeId::of::<TestAlternatePngItemView>()
        );

        let handle = workspace
            .update_in(cx, |workspace, window, cx| {
                let project_path = (worktree_id, rel_path("three.txt"));
                workspace.open_path(project_path, None, true, window, cx)
            })
            .await;
        assert!(handle.is_err());
    }
}

#[gpui::test]
async fn test_status_bar_visibility(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (workspace, _cx) =
        cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

    // Test with status bar shown (default)
    workspace.read_with(cx, |workspace, cx| {
        let visible = workspace.status_bar_visible(cx);
        assert!(visible, "Status bar should be visible by default");
    });

    // Test with status bar hidden
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.status_bar.get_or_insert_default().show = Some(false);
        });
    });

    workspace.read_with(cx, |workspace, cx| {
        let visible = workspace.status_bar_visible(cx);
        assert!(!visible, "Status bar should be hidden when show is false");
    });

    // Test with status bar shown explicitly
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |settings| {
            settings.status_bar.get_or_insert_default().show = Some(true);
        });
    });

    workspace.read_with(cx, |workspace, cx| {
        let visible = workspace.status_bar_visible(cx);
        assert!(visible, "Status bar should be visible when show is true");
    });
}
