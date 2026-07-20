use super::*;

#[gpui::test]
fn test_bundled_settings_and_themes(cx: &mut App) {
    cx.text_system()
        .add_fonts(vec![
            Assets
                .load("fonts/lilex/Lilex-Regular.ttf")
                .unwrap()
                .unwrap(),
            Assets
                .load("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf")
                .unwrap()
                .unwrap(),
        ])
        .unwrap();
    let themes = ThemeRegistry::default();
    settings::init(cx);
    theme_settings::init(theme::LoadThemes::JustBase, cx);

    let mut has_default_theme = false;
    for theme_name in themes.list().into_iter().map(|meta| meta.name) {
        let theme = themes.get(&theme_name).unwrap();
        assert_eq!(theme.name, theme_name);
        if theme.name.as_ref() == "One Dark" {
            has_default_theme = true;
        }
    }
    assert!(has_default_theme);
}

#[gpui::test]
async fn test_bundled_files_editor(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(init);

    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    cx.update(|cx| {
        cx.dispatch_action(&OpenDefaultSettings);
    });
    cx.run_until_parked();

    assert_eq!(cx.read(|cx| cx.windows().len()), 1);

    let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
    let active_editor = multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace
                .workspace()
                .update(cx, |workspace, cx| workspace.active_item_as::<Editor>(cx))
        })
        .unwrap();
    assert!(
        active_editor.is_some(),
        "Settings action should have opened an editor with the default file contents"
    );

    let active_editor = active_editor.unwrap();
    assert!(
        active_editor.read_with(cx, |editor, cx| editor.read_only(cx)),
        "Default settings should be readonly"
    );
    assert!(
        active_editor.read_with(cx, |editor, cx| editor.buffer().read(cx).read_only()),
        "The underlying buffer should also be readonly for the shipped default settings"
    );
}

#[gpui::test]
async fn test_bundled_files_reuse_existing_editor(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(init);

    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    cx.update(|cx| {
        cx.dispatch_action(&OpenDefaultSettings);
    });
    cx.run_until_parked();

    let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
    let first_item_id = multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace
                    .active_item(cx)
                    .expect("default settings should be open")
                    .item_id()
            })
        })
        .unwrap();

    cx.update(|cx| {
        cx.dispatch_action(&OpenDefaultSettings);
    });
    cx.run_until_parked();

    let (second_item_id, item_count) = multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let pane = workspace.active_pane().read(cx);
                (
                    pane.active_item()
                        .expect("default settings should still be open")
                        .item_id(),
                    pane.items_len(),
                )
            })
        })
        .unwrap();

    assert_eq!(first_item_id, second_item_id);
    assert_eq!(item_count, 1);
}

#[gpui::test]
async fn test_bundled_languages(cx: &mut TestAppContext) {
    let fs = fs::FakeFs::new(cx.background_executor.clone());
    env_logger::builder().is_test(true).try_init().ok();
    let settings = cx.update(SettingsStore::test);
    cx.set_global(settings);
    let languages = LanguageRegistry::test(cx.executor());
    let languages = Arc::new(languages);
    let node_runtime = node_runtime::NodeRuntime::unavailable();
    cx.update(|cx| {
        languages::init(languages.clone(), fs, node_runtime, cx);
    });
    for name in languages.language_names() {
        languages
            .language_for_name(name.as_ref())
            .await
            .with_context(|| format!("language name {name}"))
            .unwrap();
    }
    cx.run_until_parked();
}

pub(crate) fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
    init_test_with_state(cx, cx.update(AppState::test))
}

fn init_test_with_state(cx: &mut TestAppContext, mut app_state: Arc<AppState>) -> Arc<AppState> {
    cx.update(move |cx| {
        env_logger::builder().is_test(true).try_init().ok();

        let state = Arc::get_mut(&mut app_state).unwrap();
        state.build_window_options = build_window_options;
        app_state.languages.add(markdown_lang());

        gpui_tokio::init(cx);
        AppState::set_global(app_state.clone(), cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        audio::init(cx);
        channel::init(&app_state.client, app_state.user_store.clone(), cx);
        call::init(app_state.client.clone(), app_state.user_store.clone(), cx);
        notifications::init(app_state.client.clone(), app_state.user_store.clone(), cx);
        workspace::init(app_state.clone(), cx);
        release_channel::init(Version::new(0, 0, 0), cx);
        command_palette::init(cx);
        editor::init(cx);
        collab_ui::init(&app_state, cx);
        git_ui::init(cx);
        project_panel::init(cx);
        outline_panel::init(cx);
        terminal_view::init(cx);
        copilot_chat::init(
            app_state.fs.clone(),
            app_state.client.http_client(),
            copilot_chat::CopilotChatConfiguration::default(),
            cx,
        );
        image_viewer::init(cx);
        language_model::init(cx);
        client::RefreshLlmTokenListener::register(
            app_state.client.clone(),
            app_state.user_store.clone(),
            cx,
        );
        language_models::init(app_state.user_store.clone(), app_state.client.clone(), cx);
        web_search::init(cx);
        web_search_providers::init(app_state.client.clone(), app_state.user_store.clone(), cx);
        let prompt_builder = PromptBuilder::load(app_state.fs.clone(), false, cx);
        project::AgentRegistryStore::init_global(
            cx,
            app_state.fs.clone(),
            app_state.client.http_client(),
        );
        agent_ui::init(
            app_state.fs.clone(),
            prompt_builder,
            app_state.languages.clone(),
            true,
            false,
            cx,
        );

        repl::init(app_state.fs.clone(), cx);
        repl::notebook::init(cx);
        tasks_ui::init(cx);
        project::debugger::breakpoint_store::BreakpointStore::init(
            &app_state.client.clone().into(),
        );
        project::debugger::dap_store::DapStore::init(&app_state.client.clone().into(), cx);
        debugger_ui::init(cx);
        initialize_workspace(app_state.clone(), cx);
        search::init(cx);
        cx.set_global(workspace::PaneSearchBarCallbacks {
            setup_search_bar: |languages, toolbar, window, cx| {
                let search_bar = cx.new(|cx| search::BufferSearchBar::new(languages, window, cx));
                toolbar.update(cx, |toolbar, cx| {
                    toolbar.add_item(search_bar, window, cx);
                });
            },
            wrap_div_with_search_actions: search::buffer_search::register_pane_search_actions,
        });
        app_state
    })
}

#[track_caller]
fn assert_key_bindings_for(
    window: AnyWindowHandle,
    cx: &TestAppContext,
    actions: Vec<(&'static str, &dyn Action)>,
    line: u32,
) {
    let available_actions = cx
        .update(|cx| window.update(cx, |_, window, cx| window.available_actions(cx)))
        .unwrap();
    for (key, action) in actions {
        let bindings = cx
            .update(|cx| window.update(cx, |_, window, _| window.bindings_for_action(action)))
            .unwrap();
        // assert that...
        assert!(
            available_actions.iter().any(|bound_action| {
                // actions match...
                bound_action.partial_eq(action)
            }),
            "On {} Failed to find {}",
            line,
            action.name(),
        );
        assert!(
            // and key strokes contain the given key
            bindings
                .into_iter()
                .any(|binding| binding.keystrokes().iter().any(|k| k.key() == key)),
            "On {} Failed to find {} with key binding {}",
            line,
            action.name(),
            key
        );
    }
}
