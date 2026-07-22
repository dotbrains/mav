use super::*;

fn init_app_state(cx: &mut App) -> Arc<AppState> {
    use fs::Fs;
    use node_runtime::NodeRuntime;
    use session::Session;
    use settings::SettingsStore;

    if !cx.has_global::<SettingsStore>() {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    }

    // Use the real filesystem instead of FakeFs so we can access actual files on disk
    let fs: Arc<dyn Fs> = Arc::new(fs::RealFs::new(None, cx.background_executor().clone()));
    <dyn Fs>::set_global(fs.clone(), cx);

    let languages = Arc::new(language::LanguageRegistry::test(
        cx.background_executor().clone(),
    ));
    let clock = Arc::new(clock::FakeSystemClock::new());
    let http_client = http_client::FakeHttpClient::with_404_response();
    let client = client::Client::new(clock, http_client, cx);
    let session = cx.new(|cx| session::AppSession::new(Session::test(), cx));
    let user_store = cx.new(|cx| client::UserStore::new(client.clone(), cx));
    let workspace_store = cx.new(|cx| workspace::WorkspaceStore::new(client.clone(), cx));

    theme_settings::init(theme::LoadThemes::JustBase, cx);
    client::init(&client, cx);

    let app_state = Arc::new(AppState {
        client,
        fs,
        languages,
        user_store,
        workspace_store,
        node_runtime: NodeRuntime::unavailable(),
        build_window_options: |_, _| Default::default(),
        session,
    });
    AppState::set_global(app_state.clone(), cx);
    app_state
}
