use super::*;
use futures::channel::mpsc::UnboundedReceiver;

pub(crate) fn run(
    cx: &mut App,
    args: Args,
    app_db: db::AppDatabase,
    app_version: AppVersion,
    app_commit_sha: Option<AppCommitSha>,
    fs: Arc<RealFs>,
    git_hosting_provider_registry: Arc<GitHostingProviderRegistry>,
    open_listener: OpenListener,
    open_rx: UnboundedReceiver<RawOpenRequest>,
    crash_handler: Option<Task<crashes::Client>>,
    system_id: Task<Result<IdType>>,
    installation_id: Task<Result<IdType>>,
    session: Task<Session>,
    user_keymap_file_rx: UnboundedReceiver<String>,
    user_keymap_watcher: Task<()>,
    shell_env_loaded_rx: oneshot::Receiver<()>,
) {
    cx.set_global(app_db);
    let db_trusted_paths = match workspace::WorkspaceDb::global(cx).fetch_trusted_worktrees() {
        Ok(trusted_paths) => trusted_paths,
        Err(e) => {
            log::error!("Failed to do initial trusted worktrees fetch: {e:#}");
            HashMap::default()
        }
    };
    trusted_worktrees::init(db_trusted_paths, cx);
    menu::init();
    mav_actions::init();

    release_channel::init(app_version, cx);
    gpui_tokio::init(cx);
    if let Some(app_commit_sha) = app_commit_sha {
        AppCommitSha::set_global(app_commit_sha, cx);
    }
    settings::init(cx);
    zlog_settings::init(cx);
    mav::watch_settings_files(fs.clone(), cx);
    handle_keymap_file_changes(user_keymap_file_rx, user_keymap_watcher, cx);

    let user_agent = format!(
        "Mav/{} ({}; {})",
        AppVersion::global(cx),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let proxy_url = ProxySettings::get_global(cx).proxy_url();
    let http = {
        let _guard = Tokio::handle(cx).enter();

        ReqwestClient::proxy_and_user_agent(proxy_url, &user_agent)
            .expect("could not start HTTP client")
    };
    cx.set_http_client(Arc::new(http));

    <dyn Fs>::set_global(fs.clone(), cx);

    GitHostingProviderRegistry::set_global(git_hosting_provider_registry, cx);
    git_hosting_providers::init(cx);

    OpenListener::set_global(cx, open_listener.clone());

    extension::init(cx);
    let extension_host_proxy = ExtensionHostProxy::global(cx);

    let client = Client::production(cx);
    cx.set_http_client(client.http_client());
    let mut languages = LanguageRegistry::new(cx.background_executor().clone());
    languages.set_language_server_download_dir(paths::languages_dir().clone());
    let languages = Arc::new(languages);
    let (mut tx, rx) = watch::channel(None);
    cx.observe_global::<SettingsStore>(move |cx| {
        let settings = &ProjectSettings::get_global(cx).node;
        let options = NodeBinaryOptions {
            allow_path_lookup: !settings.ignore_system_version,
            // TODO: Expose this setting
            allow_binary_download: true,
            use_paths: settings.path.as_ref().map(|node_path| {
                let node_path = PathBuf::from(shellexpand::tilde(node_path).as_ref());
                let npm_path = settings
                    .npm_path
                    .as_ref()
                    .map(|path| PathBuf::from(shellexpand::tilde(&path).as_ref()));
                (
                    node_path.clone(),
                    npm_path.unwrap_or_else(|| {
                        let base_path = PathBuf::new();
                        node_path.parent().unwrap_or(&base_path).join("npm")
                    }),
                )
            }),
        };
        tx.send(Some(options)).log_err();
    })
    .detach();
    ui::on_new_scrollbars::<SettingsStore>(cx);

    let node_runtime = NodeRuntime::new(client.http_client(), Some(shell_env_loaded_rx), rx);

    debug_adapter_extension::init(extension_host_proxy.clone(), cx);
    languages::init(languages.clone(), fs.clone(), node_runtime.clone(), cx);
    let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
    let workspace_store = cx.new(|cx| WorkspaceStore::new(client.clone(), cx));

    language_extension::init(
        language_extension::LspAccess::ViaWorkspaces({
            let workspace_store = workspace_store.clone();
            Arc::new(move |cx: &mut App| {
                workspace_store.update(cx, |workspace_store, cx| {
                    Ok(workspace_store
                        .workspaces()
                        .filter_map(|weak| weak.upgrade())
                        .map(|workspace: gpui::Entity<workspace::Workspace>| {
                            workspace.read(cx).project().read(cx).lsp_store()
                        })
                        .collect())
                })
            })
        }),
        extension_host_proxy.clone(),
        languages.clone(),
    );

    Client::set_global(client.clone(), cx);

    mav::init(cx);
    #[cfg(target_os = "macos")]
    mav::move_to_applications::init(cx);
    project::Project::init(&client, cx);
    debugger_ui::init(cx);
    debugger_tools::init(cx);
    client::init(&client, cx);
    feature_flags::FeatureFlagStore::init(cx);

    let system_id = cx.foreground_executor().block_on(system_id).ok();
    let installation_id = cx.foreground_executor().block_on(installation_id).ok();
    let session = cx.foreground_executor().block_on(session);

    let telemetry = client.telemetry();
    telemetry.start(
        system_id.as_ref().map(|id| id.to_string()),
        installation_id.as_ref().map(|id| id.to_string()),
        session.id().to_owned(),
        cx,
    );
    cx.subscribe(&user_store, {
        let telemetry = telemetry.clone();
        move |_, evt: &client::user::Event, cx| match evt {
            client::user::Event::PrivateUserInfoUpdated => {
                if let Some(crash_client) = cx.try_global::<CrashHandler>() {
                    crashes::set_user_info(
                        &crash_client.0,
                        crashes::UserInfo {
                            metrics_id: telemetry.metrics_id().map(|s| s.to_string()),
                            is_staff: telemetry.is_staff(),
                        },
                    );
                }
            }
            _ => {}
        }
    })
    .detach();

    let is_new_install = matches!(&installation_id, Some(IdType::New(_)));

    // We should rename these in the future to `first app open`, `first app open for release channel`, and `app open`
    if let (Some(system_id), Some(installation_id)) = (&system_id, &installation_id) {
        match (&system_id, &installation_id) {
            (IdType::New(_), IdType::New(_)) => {
                telemetry::event!("App First Opened");
                telemetry::event!("App First Opened For Release Channel");
            }
            (IdType::Existing(_), IdType::New(_)) => {
                telemetry::event!("App First Opened For Release Channel");
            }
            (_, IdType::Existing(_)) => {
                telemetry::event!("App Opened");
            }
        }
    }
    let app_session = cx.new(|cx| AppSession::new(session, cx));

    let app_state = Arc::new(AppState {
        languages,
        client: client.clone(),
        user_store,
        fs: fs.clone(),
        build_window_options,
        workspace_store,
        node_runtime,
        session: app_session,
    });
    AppState::set_global(app_state.clone(), cx);

    auto_update::init(client.clone(), cx);
    dap_adapters::init(cx);
    auto_update_ui::init(cx);
    reliability::init(client.clone(), cx);
    extension_host::init(
        extension_host_proxy.clone(),
        app_state.fs.clone(),
        app_state.client.clone(),
        app_state.node_runtime.clone(),
        cx,
    );

    theme_settings::init(theme::LoadThemes::All(Box::new(Assets)), cx);
    eager_load_active_theme_and_icon_theme(fs.clone(), cx);
    theme_extension::init(
        extension_host_proxy,
        ThemeRegistry::global(cx),
        cx.background_executor().clone(),
    );
    command_palette::init(cx);
    let copilot_chat_configuration = copilot_chat::CopilotChatConfiguration {
        enterprise_uri: language::language_settings::all_language_settings(None, cx)
            .edit_predictions
            .copilot
            .enterprise_uri
            .clone(),
    };
    copilot_chat::init(
        app_state.fs.clone(),
        app_state.client.http_client(),
        copilot_chat_configuration,
        cx,
    );

    copilot_ui::init(&app_state, cx);
    language_model::init(cx);
    RefreshLlmTokenListener::register(app_state.client.clone(), app_state.user_store.clone(), cx);
    language_models::init(app_state.user_store.clone(), app_state.client.clone(), cx);
    acp_tools::init(cx);
    mav::telemetry_log::init(cx);
    mav::remote_debug::init(cx);
    edit_prediction_ui::init(cx);
    web_search::init(cx);
    web_search_providers::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    snippet_provider::init(cx);
    edit_prediction_registry::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    let prompt_builder = PromptBuilder::load(app_state.fs.clone(), stdout_is_a_pty(), cx);
    project::AgentRegistryStore::init_global(
        cx,
        app_state.fs.clone(),
        app_state.client.http_client(),
    );
    agent_ui::init(
        app_state.fs.clone(),
        prompt_builder,
        app_state.languages.clone(),
        is_new_install,
        false,
        cx,
    );
    mav::watch_user_agents_md(app_state.fs.clone(), cx);

    repl::init(app_state.fs.clone(), cx);
    recent_projects::init(cx);
    dev_container::init(cx);

    load_embedded_fonts(cx);

    editor::init(cx);
    image_viewer::init(cx);
    repl::notebook::init(cx);
    diagnostics::init(cx);

    audio::init(cx);
    workspace::init(app_state.clone(), cx);
    ui_prompt::init(cx);

    go_to_line::init(cx);
    file_finder::init(cx);
    tab_switcher::init(cx);
    outline::init(cx);
    project_symbols::init(cx);
    project_panel::init(cx);
    outline_panel::init(cx);
    tasks_ui::init(cx);
    snippets_ui::init(cx);
    channel::init(&app_state.client.clone(), app_state.user_store.clone(), cx);
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
    vim::init(cx);
    terminal_view::init(cx);
    journal::init(app_state.clone(), cx);
    encoding_selector::init(cx);
    language_selector::init(cx);
    line_ending_selector::init(cx);
    toolchain_selector::init(cx);
    theme_selector::init(cx);
    settings_profile_selector::init(cx);
    language_tools::init(cx);
    call::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    notifications::init(app_state.client.clone(), app_state.user_store.clone(), cx);
    collab_ui::init(&app_state, cx);
    git_ui::init(cx);
    feedback::init(cx);
    markdown_preview::init(cx);
    csv_preview::init(cx);
    svg_preview::init(cx);
    onboarding::init(cx);
    settings_ui::init(cx);
    keymap_editor::init(cx);
    extensions_ui::init(cx);
    edit_prediction::init(cx);
    inspector_ui::init(app_state.clone(), cx);
    json_schema_store::init(cx);
    miniprofiler_ui::init(*STARTUP_TIME.get().unwrap(), cx);
    which_key::init(cx);
    #[cfg(target_os = "windows")]
    etw_tracing::init(cx);

    cx.observe_global::<SettingsStore>({
        let http = app_state.client.http_client();
        let client = app_state.client.clone();
        move |cx| {
            for &mut window in cx.windows().iter_mut() {
                let background_appearance = cx.theme().window_background_appearance();
                window
                    .update(cx, |_, window, _| {
                        window.set_background_appearance(background_appearance)
                    })
                    .ok();
            }

            cx.set_text_rendering_mode(
                match WorkspaceSettings::get_global(cx).text_rendering_mode {
                    settings::TextRenderingMode::PlatformDefault => {
                        gpui::TextRenderingMode::PlatformDefault
                    }
                    settings::TextRenderingMode::Subpixel => gpui::TextRenderingMode::Subpixel,
                    settings::TextRenderingMode::Grayscale => gpui::TextRenderingMode::Grayscale,
                },
            );

            let new_host = &client::ClientSettings::get_global(cx).server_url;
            if &http.base_url() != new_host {
                http.set_base_url(new_host);
                if client.status().borrow().is_connected() {
                    client.reconnect(&cx.to_async());
                }
            }
        }
    })
    .detach();
    app_state.languages.set_theme(cx.theme().clone());
    cx.observe_global::<GlobalTheme>({
        let languages = app_state.languages.clone();
        move |cx| {
            languages.set_theme(cx.theme().clone());
        }
    })
    .detach();
    telemetry::event!(
        "Settings Changed",
        setting = "theme",
        value = cx.theme().name.to_string()
    );
    telemetry::event!(
        "Settings Changed",
        setting = "keymap",
        value = BaseKeymap::get_global(cx).to_string()
    );
    telemetry.flush_events().detach();

    let fs = app_state.fs.clone();
    load_user_themes_in_background(fs.clone(), cx);
    watch_themes(fs.clone(), cx);
    #[cfg(debug_assertions)]
    watch_languages(fs.clone(), app_state.languages.clone(), cx);

    let menus = app_menus(cx);
    cx.set_menus(menus);

    if let Some(mut crash_handler) = crash_handler {
        let crash_handler2 = block_on(poll_once(&mut crash_handler));
        match crash_handler2 {
            Some(crash_handler) => {
                cx.set_global(CrashHandler(crash_handler));
            }
            None => {
                cx.spawn(async move |cx| {
                    let client1 = crash_handler.await;
                    cx.update(|cx| {
                        cx.set_global(CrashHandler(client1));
                    });
                })
                .detach();
            }
        }
    }

    initialize_workspace(app_state.clone(), cx);

    cx.activate(true);

    cx.spawn({
        let client = app_state.client.clone();
        async move |cx| authenticate(client, cx).await
    })
    .detach_and_log_err(cx);

    startup_open::handle_initial_open_requests(args, open_listener, open_rx, app_state, cx);
}
