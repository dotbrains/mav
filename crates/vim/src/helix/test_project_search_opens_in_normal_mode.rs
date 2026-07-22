use super::*;

#[gpui::test]
async fn test_project_search_opens_in_normal_mode(cx: &mut gpui::TestAppContext) {
    VimTestContext::init(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a.rs": "// File A.",
            "file_b.rs": "// File B.",
        }),
    )
    .await;

    let project = project::Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    cx.update(|cx| {
        VimTestContext::init_keybindings(true, cx);
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |store| store.helix_mode = Some(true));
        })
    });

    let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

    workspace.update_in(cx, |workspace, window, cx| {
        ProjectSearchView::deploy_search(workspace, &DeploySearch::default(), window, cx)
    });

    let search_view = workspace.update_in(cx, |workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .items()
            .find_map(|item| item.downcast::<ProjectSearchView>())
            .expect("Project search view should be active")
    });

    project_search::perform_project_search(&search_view, "File A", cx);

    search_view.update(cx, |search_view, cx| {
        let vim_mode = search_view
            .results_editor()
            .read(cx)
            .addon::<VimAddon>()
            .map(|addon| addon.entity.read(cx).mode);

        assert_eq!(vim_mode, Some(Mode::HelixNormal));
    });
}
