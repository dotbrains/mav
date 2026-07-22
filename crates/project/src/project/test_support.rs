use super::*;

impl Project {
    #[cfg(feature = "test-support")]
    pub fn client_subscriptions(&self) -> &Vec<client::Subscription> {
        &self.client_subscriptions
    }

    #[cfg(feature = "test-support")]
    pub async fn example(
        root_paths: impl IntoIterator<Item = &Path>,
        cx: &mut AsyncApp,
    ) -> Entity<Project> {
        use clock::FakeSystemClock;

        let fs = Arc::new(RealFs::new(None, cx.background_executor().clone()));
        let languages = LanguageRegistry::test(cx.background_executor().clone());
        let clock = Arc::new(FakeSystemClock::new());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let client = cx.update(|cx| client::Client::new(clock, http_client.clone(), cx));
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        let project = cx.update(|cx| {
            Project::local(
                client,
                node_runtime::NodeRuntime::unavailable(),
                user_store,
                Arc::new(languages),
                fs,
                None,
                LocalProjectFlags {
                    init_worktree_trust: false,
                    ..Default::default()
                },
                cx,
            )
        });
        for path in root_paths {
            let (tree, _): (Entity<Worktree>, _) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(path, true, cx)
                })
                .await
                .unwrap();
            tree.read_with(cx, |tree, _| tree.as_local().unwrap().scan_complete())
                .await;
        }
        project
    }

    #[cfg(feature = "test-support")]
    pub async fn test(
        fs: Arc<dyn Fs>,
        root_paths: impl IntoIterator<Item = &Path>,
        cx: &mut gpui::TestAppContext,
    ) -> Entity<Project> {
        Self::test_project(fs, root_paths, false, cx).await
    }

    #[cfg(feature = "test-support")]
    pub async fn test_with_worktree_trust(
        fs: Arc<dyn Fs>,
        root_paths: impl IntoIterator<Item = &Path>,
        cx: &mut gpui::TestAppContext,
    ) -> Entity<Project> {
        Self::test_project(fs, root_paths, true, cx).await
    }

    #[cfg(feature = "test-support")]
    async fn test_project(
        fs: Arc<dyn Fs>,
        root_paths: impl IntoIterator<Item = &Path>,
        init_worktree_trust: bool,
        cx: &mut gpui::TestAppContext,
    ) -> Entity<Project> {
        use clock::FakeSystemClock;

        let languages = LanguageRegistry::test(cx.executor());
        let clock = Arc::new(FakeSystemClock::new());
        let http_client = http_client::FakeHttpClient::with_404_response();
        let client = cx.update(|cx| client::Client::new(clock, http_client.clone(), cx));
        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        let project = cx.update(|cx| {
            Project::local(
                client,
                node_runtime::NodeRuntime::unavailable(),
                user_store,
                Arc::new(languages),
                fs,
                None,
                LocalProjectFlags {
                    init_worktree_trust,
                    ..Default::default()
                },
                cx,
            )
        });
        for path in root_paths {
            let (tree, _) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(path, true, cx)
                })
                .await
                .unwrap();

            tree.read_with(cx, |tree, _| tree.as_local().unwrap().scan_complete())
                .await;
        }
        project
    }

    /// Transitions a local test project into the `Collab` client state so that
    /// `is_via_collab()` returns `true`. Use only in tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn mark_as_collab_for_testing(&mut self) {
        self.client_state = ProjectClientState::Collab {
            sharing_has_stopped: false,
            capability: Capability::ReadWrite,
            remote_id: 0,
            replica_id: clock::ReplicaId::new(1),
        };
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn add_test_remote_worktree(
        &mut self,
        abs_path: &str,
        cx: &mut Context<Self>,
    ) -> Entity<Worktree> {
        use rpc::NoopProtoClient;
        use util::paths::PathStyle;

        let root_name = std::path::Path::new(abs_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let client = AnyProtoClient::new(NoopProtoClient::new());
        let worktree = Worktree::remote(
            0,
            ReplicaId::new(1),
            proto::WorktreeMetadata {
                id: 100 + self.visible_worktrees(cx).count() as u64,
                root_name,
                visible: true,
                abs_path: abs_path.to_string(),
                root_repo_common_dir: None,
            },
            client,
            PathStyle::Posix,
            cx,
        );
        self.worktree_store
            .update(cx, |store, cx| store.add(&worktree, cx));
        worktree
    }
}
