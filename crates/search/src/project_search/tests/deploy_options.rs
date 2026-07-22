use super::*;

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
