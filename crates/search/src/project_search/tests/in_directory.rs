use super::*;

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
