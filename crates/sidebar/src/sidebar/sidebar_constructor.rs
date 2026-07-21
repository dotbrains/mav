use super::*;

impl Sidebar {
    pub fn new(
        multi_workspace: Entity<MultiWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus_in(&focus_handle, window, Self::focus_in)
            .detach();

        AgentThreadWorktreeLabelFlag::watch(cx);

        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Search threads…", window, cx);
            editor
        });
        let thread_rename_editor = cx.new(|cx| Editor::single_line(window, cx));
        let sidebar_chrome = cx.new(|cx| {
            let workspace = multi_workspace.read(cx).workspace().clone();
            title_bar::SidebarChrome::new(
                "sidebar-title-bar-controls",
                workspace,
                Some(multi_workspace.downgrade()),
                window,
                cx,
            )
        });

        cx.subscribe_in(
            &multi_workspace,
            window,
            |this, _multi_workspace, event: &MultiWorkspaceEvent, window, cx| match event {
                MultiWorkspaceEvent::ActiveWorkspaceChanged { .. } => {
                    let workspace = _multi_workspace.read(cx).workspace().clone();
                    this.sidebar_chrome = cx.new(|cx| {
                        title_bar::SidebarChrome::new(
                            "sidebar-title-bar-controls",
                            workspace,
                            Some(_multi_workspace.downgrade()),
                            window,
                            cx,
                        )
                    });
                    this.sync_active_entry_from_active_workspace(cx);
                    this.replace_archived_panel_thread(window, cx);
                    this.schedule_update_entries(false, cx);
                }
                MultiWorkspaceEvent::WorkspaceAdded(workspace) => {
                    this.subscribe_to_workspace(workspace, window, cx);
                    this.schedule_update_entries(false, cx);
                }
                MultiWorkspaceEvent::WorkspaceRemoved(_)
                | MultiWorkspaceEvent::ProjectGroupsChanged => {
                    this.schedule_update_entries(false, cx);
                }
            },
        )
        .detach();

        cx.subscribe(&filter_editor, |this: &mut Self, _, event, cx| {
            if let editor::EditorEvent::BufferEdited = event {
                let query = this.filter_editor.read(cx).text(cx);
                if !query.is_empty() {
                    this.selection.take();
                }
                this.schedule_update_entries(!query.is_empty(), cx);
            }
        })
        .detach();

        cx.subscribe_in(
            &thread_rename_editor,
            window,
            |this, title_editor, event, window, cx| {
                this.handle_thread_rename_editor_event(title_editor, event, window, cx);
            },
        )
        .detach();

        cx.observe(&ThreadMetadataStore::global(cx), |this, _store, cx| {
            this.schedule_update_entries(false, cx);
        })
        .detach();

        cx.observe(
            &TerminalThreadMetadataStore::global(cx),
            |this, _store, cx| {
                this.schedule_update_entries(false, cx);
            },
        )
        .detach();

        let channels_with_threads = channels_with_threads(cx);
        cx.spawn(async move |this, cx| {
            let channels = channels_with_threads.await;
            this.update(cx, |this, cx| {
                this.cross_channel_import_channels = channels;
                cx.notify();
            })
            .ok();
        })
        .detach();

        let deferred_multi_workspace = multi_workspace.downgrade();
        cx.defer_in(window, move |this, window, cx| {
            if let Some(multi_workspace) = deferred_multi_workspace.upgrade() {
                let workspaces: Vec<_> = multi_workspace.read(cx).workspaces().cloned().collect();
                for workspace in &workspaces {
                    this.subscribe_to_workspace(workspace, window, cx);
                }
            }
            this.schedule_update_entries(false, cx);
        });

        Self {
            multi_workspace: multi_workspace.downgrade(),
            width: DEFAULT_WIDTH,
            focus_handle,
            filter_editor,
            thread_rename_editor,
            list_state: ListState::new(0, gpui::ListAlignment::Top, px(1000.)),
            contents: SidebarContents::default(),
            selection: None,
            active_entry: None,
            hovered_thread_index: None,
            renaming_thread_id: None,
            regenerating_titles: HashSet::new(),
            suppress_next_rename_edit: false,

            thread_last_accessed: HashMap::new(),
            terminal_last_accessed: HashMap::new(),
            thread_switcher: None,
            _thread_switcher_subscriptions: Vec::new(),
            pending_thread_activation: None,
            live_thread_statuses: HashMap::new(),
            draft_kinds: HashMap::new(),
            view: SidebarView::default(),
            restoring_tasks: HashMap::new(),
            agent_options_menu_handle: PopoverMenuHandle::default(),
            recent_projects_popover_handle: PopoverMenuHandle::default(),
            sidebar_chrome,
            project_header_menu_handles: HashMap::new(),
            project_header_new_thread_menu_handles: HashMap::new(),
            project_header_menu_ix: None,
            worktree_default_branches: HashMap::new(),
            _subscriptions: Vec::new(),
            _draft_editor_observations: Vec::new(),
            update_task: None,
            import_banners_use_verbose_labels: None,
            cross_channel_import_channels: Vec::new(),
        }
    }
}
