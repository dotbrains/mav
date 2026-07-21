use super::*;

impl Sidebar {
    pub(super) fn record_thread_access(&mut self, id: &ThreadId) {
        self.thread_last_accessed.insert(*id, Utc::now());
    }

    pub(super) fn record_terminal_access(&mut self, id: TerminalId) {
        self.terminal_last_accessed.insert(id, Utc::now());
    }

    pub(super) fn record_thread_interacted(
        &mut self,
        thread_id: &agent_ui::ThreadId,
        cx: &mut App,
    ) {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.update_interacted_at(thread_id, Utc::now(), cx);
        })
    }

    pub(super) fn thread_display_time(metadata: &ThreadMetadata) -> DateTime<Utc> {
        metadata.interacted_at.unwrap_or(metadata.updated_at)
    }

    pub(super) fn push_entries_by_display_time(
        entries: &mut Vec<ListEntry>,
        terminals: Vec<TerminalEntry>,
        threads: Vec<Arc<ThreadEntry>>,
        current_session_ids: &mut HashSet<acp::SessionId>,
        current_thread_ids: &mut HashSet<agent_ui::ThreadId>,
    ) {
        fn display_time(entry: &ListEntry) -> DateTime<Utc> {
            match entry {
                ListEntry::Thread(thread) if thread.draft == Some(DraftKind::Empty) => {
                    DateTime::<Utc>::MAX_UTC
                }
                ListEntry::Thread(thread) => Sidebar::thread_display_time(&thread.metadata),
                ListEntry::Terminal(terminal) => terminal.metadata.created_at,
                ListEntry::ProjectHeader { .. } => unreachable!(),
            }
        }

        let row_entries = terminals
            .into_iter()
            .map(ListEntry::Terminal)
            .chain(threads.into_iter().map(ListEntry::Thread))
            .sorted_by_key(|right| std::cmp::Reverse(display_time(right)));

        for entry in row_entries {
            if let ListEntry::Thread(thread) = &entry {
                if let Some(session_id) = &thread.metadata.session_id {
                    current_session_ids.insert(session_id.clone());
                }
                current_thread_ids.insert(thread.metadata.thread_id);
            }
            entries.push(entry);
        }
    }

    /// The sort order used by the ctrl-tab switcher
    fn switcher_entry_cmp(
        &self,
        left: &ThreadSwitcherEntry,
        right: &ThreadSwitcherEntry,
    ) -> Ordering {
        let sort_time = |entry: &ThreadSwitcherEntry| match entry {
            ThreadSwitcherEntry::Thread(entry) => self
                .thread_last_accessed
                .get(&entry.metadata.thread_id)
                .copied()
                .or(entry.metadata.interacted_at)
                .unwrap_or(entry.metadata.updated_at),
            ThreadSwitcherEntry::Terminal(entry) => self
                .terminal_last_accessed
                .get(&entry.metadata.terminal_id)
                .copied()
                .unwrap_or(entry.metadata.created_at),
        };

        // .reverse() = most recent first
        sort_time(left).cmp(&sort_time(right)).reverse()
    }

    fn mru_entries_for_switcher(&self, cx: &App) -> Vec<ThreadSwitcherEntry> {
        let mut current_header_label: Option<SharedString> = None;
        let mut current_header_key: Option<ProjectGroupKey> = None;
        let mut entries: Vec<ThreadSwitcherEntry> = self
            .contents
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ListEntry::ProjectHeader { label, key, .. } => {
                    current_header_label = Some(label.clone());
                    current_header_key = Some(key.clone());
                    None
                }
                ListEntry::Thread(thread) => {
                    if thread.draft == Some(DraftKind::Empty) {
                        return None;
                    }
                    let workspace = match &thread.workspace {
                        ThreadEntryWorkspace::Open(workspace) => Some(workspace.clone()),
                        ThreadEntryWorkspace::Closed { .. } => {
                            current_header_key.as_ref().and_then(|key| {
                                self.multi_workspace.upgrade().and_then(|mw| {
                                    mw.read(cx).workspace_for_paths(
                                        key.path_list(),
                                        key.host().as_ref(),
                                        cx,
                                    )
                                })
                            })
                        }
                    }?;
                    let notified = self.contents.is_thread_notified(&thread.metadata.thread_id);
                    let timestamp: SharedString =
                        format_history_entry_timestamp(Self::thread_display_time(&thread.metadata))
                            .into();
                    Some(ThreadSwitcherEntry::Thread(ThreadSwitcherThreadEntry {
                        title: thread.metadata.display_title(),
                        icon: thread.icon,
                        icon_from_external_svg: thread.icon_from_external_svg.clone(),
                        status: thread.status,
                        metadata: thread.metadata.clone(),
                        workspace,
                        project_name: current_header_label.clone(),
                        worktrees: thread
                            .worktrees
                            .iter()
                            .cloned()
                            .map(|mut wt| {
                                wt.highlight_positions = Vec::new();
                                wt
                            })
                            .collect(),
                        diff_stats: thread.diff_stats,
                        is_draft: thread.draft.is_some(),
                        is_title_generating: thread.is_title_generating,
                        notified,
                        timestamp,
                    }))
                }
                ListEntry::Terminal(terminal) => {
                    let timestamp: SharedString =
                        format_history_entry_timestamp(terminal.metadata.created_at).into();
                    Some(ThreadSwitcherEntry::Terminal(ThreadSwitcherTerminalEntry {
                        metadata: terminal.metadata.clone(),
                        workspace: terminal.workspace.clone(),
                        project_name: current_header_label.clone(),
                        worktrees: terminal
                            .worktrees
                            .iter()
                            .cloned()
                            .map(|mut wt| {
                                wt.highlight_positions = Vec::new();
                                wt
                            })
                            .collect(),
                        notified: self
                            .contents
                            .is_terminal_notified(terminal.metadata.terminal_id),
                        timestamp,
                    }))
                }
            })
            .collect();

        entries.sort_by(|a, b| self.switcher_entry_cmp(a, b));

        entries
    }

    fn dismiss_thread_switcher(&mut self, cx: &mut Context<Self>) {
        self.thread_switcher = None;
        self._thread_switcher_subscriptions.clear();
        if let Some(mw) = self.multi_workspace.upgrade() {
            mw.update(cx, |mw, cx| {
                mw.set_sidebar_overlay(None, cx);
            });
        }
    }

    pub(super) fn on_toggle_thread_switcher(
        &mut self,
        action: &ToggleThreadSwitcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_thread_switcher_impl(action.select_last, window, cx);
    }

    fn preview_switcher_selection(
        &mut self,
        selection: &ThreadSwitcherSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match selection {
            ThreadSwitcherSelection::Thread {
                metadata,
                workspace,
            } => {
                if let Some(multi_workspace) = self.multi_workspace.upgrade() {
                    multi_workspace.update(cx, |multi_workspace, cx| {
                        multi_workspace.activate(workspace.clone(), None, window, cx);
                    });
                }
                self.active_entry = Some(ActiveEntry::Thread {
                    thread_id: metadata.thread_id,
                    session_id: metadata.session_id.clone(),
                    workspace: workspace.clone(),
                });
                self.update_entries(cx);
                Self::load_agent_thread_in_workspace(workspace, metadata, false, window, cx);
            }
            ThreadSwitcherSelection::Terminal {
                metadata,
                workspace,
            } => {
                if let ThreadEntryWorkspace::Open(workspace) = workspace {
                    if let Some(multi_workspace) = self.multi_workspace.upgrade() {
                        multi_workspace.update(cx, |multi_workspace, cx| {
                            multi_workspace.activate(workspace.clone(), None, window, cx);
                        });
                    }
                    self.active_entry = Some(ActiveEntry::Terminal {
                        terminal_id: metadata.terminal_id,
                        workspace: workspace.clone(),
                    });
                    self.update_entries(cx);
                    Self::load_agent_terminal_in_workspace(workspace, metadata, false, window, cx);
                }
            }
        }
    }

    fn confirm_switcher_selection(
        &mut self,
        selection: &ThreadSwitcherSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match selection {
            ThreadSwitcherSelection::Thread {
                metadata,
                workspace,
            } => {
                if let Some(multi_workspace) = self.multi_workspace.upgrade() {
                    multi_workspace.update(cx, |multi_workspace, cx| {
                        multi_workspace.activate(workspace.clone(), None, window, cx);
                        multi_workspace.retain_active_workspace(cx);
                    });
                }
                self.record_thread_access(&metadata.thread_id);
                self.active_entry = Some(ActiveEntry::Thread {
                    thread_id: metadata.thread_id,
                    session_id: metadata.session_id.clone(),
                    workspace: workspace.clone(),
                });
                self.update_entries(cx);
                self.dismiss_thread_switcher(cx);
                Self::load_agent_thread_in_workspace(workspace, metadata, true, window, cx);
            }
            ThreadSwitcherSelection::Terminal {
                metadata,
                workspace,
            } => {
                self.dismiss_thread_switcher(cx);
                self.activate_terminal_entry(metadata.clone(), workspace.clone(), true, window, cx);
            }
        }
    }

    pub(super) fn toggle_thread_switcher_impl(
        &mut self,
        select_last: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread_switcher) = &self.thread_switcher {
            thread_switcher.update(cx, |switcher, cx| {
                if select_last {
                    switcher.select_last(cx);
                } else {
                    switcher.cycle_selection(cx);
                }
            });
            return;
        }

        let entries = self.mru_entries_for_switcher(cx);
        if entries.len() < 2 {
            return;
        }

        let weak_multi_workspace = self.multi_workspace.clone();

        // Snapshot the active entry (thread or terminal) so dismissal can
        // restore it.
        let original_active_entry = self.active_entry.clone();
        let original_metadata = match &original_active_entry {
            Some(ActiveEntry::Thread { thread_id, .. }) => {
                entries.iter().find_map(|entry| match entry {
                    ThreadSwitcherEntry::Thread(entry)
                        if *thread_id == entry.metadata.thread_id =>
                    {
                        Some(entry.metadata.clone())
                    }
                    _ => None,
                })
            }
            _ => None,
        };
        let original_workspace = self
            .multi_workspace
            .upgrade()
            .map(|mw| mw.read(cx).workspace().clone());

        let thread_switcher = cx.new(|cx| ThreadSwitcher::new(entries, select_last, window, cx));

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe_in(&thread_switcher, window, {
            let thread_switcher = thread_switcher.clone();
            move |this, _emitter, event: &ThreadSwitcherEvent, window, cx| match event {
                ThreadSwitcherEvent::Preview(selection) => {
                    this.preview_switcher_selection(selection, window, cx);
                    let focus = thread_switcher.focus_handle(cx);
                    window.focus(&focus, cx);
                }
                ThreadSwitcherEvent::Confirmed(selection) => {
                    this.confirm_switcher_selection(selection, window, cx);
                }
                ThreadSwitcherEvent::Dismissed => {
                    if let Some(mw) = weak_multi_workspace.upgrade() {
                        if let Some(original_ws) = &original_workspace {
                            mw.update(cx, |mw, cx| {
                                mw.activate(original_ws.clone(), None, window, cx);
                            });
                        }
                    }
                    match &original_active_entry {
                        Some(ActiveEntry::Thread { .. }) => {
                            if let (Some(metadata), Some(original_ws)) =
                                (&original_metadata, &original_workspace)
                            {
                                this.active_entry = Some(ActiveEntry::Thread {
                                    thread_id: metadata.thread_id,
                                    session_id: metadata.session_id.clone(),
                                    workspace: original_ws.clone(),
                                });
                                this.update_entries(cx);
                                Self::load_agent_thread_in_workspace(
                                    original_ws,
                                    metadata,
                                    false,
                                    window,
                                    cx,
                                );
                            }
                        }
                        Some(ActiveEntry::Terminal {
                            terminal_id,
                            workspace,
                        }) => {
                            let terminal_id = *terminal_id;
                            let workspace = workspace.clone();
                            this.active_entry = Some(ActiveEntry::Terminal {
                                terminal_id,
                                workspace: workspace.clone(),
                            });
                            this.update_entries(cx);
                            workspace.update(cx, |workspace, cx| {
                                if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                                    panel.update(cx, |panel, cx| {
                                        panel.activate_terminal(terminal_id, false, window, cx);
                                    });
                                }
                            });
                        }
                        None => {}
                    }
                    this.dismiss_thread_switcher(cx);
                }
            }
        }));

        subscriptions.push(cx.subscribe_in(
            &thread_switcher,
            window,
            |this, _emitter, _event: &gpui::DismissEvent, _window, cx| {
                this.dismiss_thread_switcher(cx);
            },
        ));

        let focus = thread_switcher.focus_handle(cx);
        let overlay_view = gpui::AnyView::from(thread_switcher.clone());

        // Replay the initial preview that was emitted during construction
        // before subscriptions were wired up.
        let initial_preview = thread_switcher
            .read(cx)
            .selected_entry()
            .map(ThreadSwitcherEntry::selection);

        self.thread_switcher = Some(thread_switcher);
        self._thread_switcher_subscriptions = subscriptions;
        if let Some(mw) = self.multi_workspace.upgrade() {
            mw.update(cx, |mw, cx| {
                mw.set_sidebar_overlay(Some(overlay_view), cx);
            });
        }

        if let Some(selection) = initial_preview {
            self.preview_switcher_selection(&selection, window, cx);
        }

        window.focus(&focus, cx);
    }
}
