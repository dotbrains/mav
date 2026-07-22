use super::*;

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
