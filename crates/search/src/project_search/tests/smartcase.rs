use super::*;

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
