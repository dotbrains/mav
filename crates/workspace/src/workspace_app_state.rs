use super::*;

pub struct AppState {
    pub languages: Arc<LanguageRegistry>,
    pub client: Arc<Client>,
    pub user_store: Entity<UserStore>,
    pub workspace_store: Entity<WorkspaceStore>,
    pub fs: Arc<dyn fs::Fs>,
    pub build_window_options: fn(Option<Uuid>, &mut App) -> WindowOptions,
    pub node_runtime: NodeRuntime,
    pub session: Entity<AppSession>,
}

struct GlobalAppState(Arc<AppState>);

impl Global for GlobalAppState {}

impl AppState {
    #[track_caller]
    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalAppState>().0.clone()
    }
    pub fn try_global(cx: &App) -> Option<Arc<Self>> {
        cx.try_global::<GlobalAppState>()
            .map(|state| state.0.clone())
    }
    pub fn set_global(state: Arc<AppState>, cx: &mut App) {
        cx.set_global(GlobalAppState(state));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(cx: &mut App) -> Arc<Self> {
        use fs::Fs;
        use node_runtime::NodeRuntime;
        use session::Session;
        use settings::SettingsStore;

        if !cx.has_global::<SettingsStore>() {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        }

        let fs = fs::FakeFs::new(cx.background_executor().clone());
        <dyn Fs>::set_global(fs.clone(), cx);
        let languages = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        let clock = Arc::new(clock::FakeSystemClock::new());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let client = Client::new(clock, http_client, cx);
        let session = cx.new(|cx| AppSession::new(Session::test(), cx));
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        let workspace_store = cx.new(|cx| WorkspaceStore::new(client.clone(), cx));

        theme_settings::init(theme::LoadThemes::JustBase, cx);
        client::init(&client, cx);

        Arc::new(Self {
            client,
            fs,
            languages,
            user_store,
            workspace_store,
            node_runtime: NodeRuntime::unavailable(),
            build_window_options: |_, _| Default::default(),
            session,
        })
    }
}
