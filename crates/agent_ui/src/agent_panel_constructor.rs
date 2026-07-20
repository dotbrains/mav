use super::*;

impl AgentPanel {
    pub(crate) fn new(workspace: &Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fs = workspace.app_state().fs.clone();
        let user_store = workspace.app_state().user_store.clone();
        let project = workspace.project();
        let language_registry = project.read(cx).languages().clone();
        let client = workspace.client().clone();
        let workspace_id = workspace.database_id();
        let workspace = workspace.weak_handle();

        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(project.read(cx).context_server_store(), cx));

        let thread_store = ThreadStore::global(cx);

        let base_view = BaseView::Uninitialized;

        let weak_panel = cx.entity().downgrade();
        let onboarding = cx.new(|cx| {
            AgentPanelOnboarding::new(
                user_store.clone(),
                client,
                move |_window, cx| {
                    weak_panel
                        .update(cx, |panel, cx| {
                            panel.dismiss_ai_onboarding(cx);
                        })
                        .ok();
                },
                cx,
            )
        });

        // Subscribe to extension events to sync agent servers when extensions change.
        let extension_subscription = ExtensionStore::try_global(cx).map(|store| {
            cx.subscribe(&store, |this, _source, event, cx| match event {
                extension_host::Event::ExtensionUninstalled(id) => {
                    this.migrate_agent_server_from_extensions(id.clone(), cx);
                }
                _ => {}
            })
        });

        let connection_store = cx.new(|cx| AgentConnectionStore::new(project.clone(), cx));
        let _project_subscription =
            cx.subscribe(&project, |this, _project, event, cx| match event {
                project::Event::WorktreeAdded(_)
                | project::Event::WorktreeRemoved(_)
                | project::Event::WorktreeOrderChanged
                | project::Event::WorktreePathsChanged { .. } => {
                    this.ensure_native_agent_connection(cx);
                    this.update_thread_work_dirs(cx);
                    this.persist_all_terminal_metadata(cx);
                    cx.notify();
                }
                _ => {}
            });

        let _thread_metadata_store_subscription = cx.subscribe(
            &ThreadMetadataStore::global(cx),
            |this, _store, event, cx| {
                let ThreadMetadataStoreEvent::ThreadArchived(thread_id) = event;
                if this.retained_threads.remove(thread_id).is_some() {
                    cx.notify();
                }
            },
        );

        cx.on_release(|this, cx| {
            this.dismiss_all_terminal_notifications(cx);
        })
        .detach();

        let panel = Self {
            workspace_id,
            base_view,
            last_created_entry_kind: AgentPanelEntryKind::Thread,
            overlay_view: None,
            workspace,
            user_store,
            project: project.clone(),
            fs: fs.clone(),
            language_registry,
            connection_store,
            configuration: None,
            configuration_subscription: None,
            focus_handle: cx.focus_handle(),
            context_server_registry,
            draft_thread: None,
            retained_threads: HashMap::default(),
            terminals: HashMap::default(),
            pending_terminal_spawn: None,
            new_thread_menu_handle: PopoverMenuHandle::default(),
            agent_panel_menu_handle: PopoverMenuHandle::default(),

            _extension_subscription: extension_subscription,
            _project_subscription,
            zoomed: false,
            pending_serialization: None,
            new_user_onboarding: onboarding,
            thread_store,
            selected_agent: Agent::default(),
            _thread_view_subscription: None,
            _active_thread_focus_subscription: None,
            new_user_onboarding_upsell_dismissed: AtomicBool::new(OnboardingUpsell::dismissed(cx)),
            _base_view_observation: None,
            _draft_editor_observation: None,
            _active_draft_reclaim_observation: None,
            _thread_metadata_store_subscription,
            last_context_source: None,
            is_active: false,
        };

        panel.ensure_native_agent_connection(cx);
        panel
    }
}
