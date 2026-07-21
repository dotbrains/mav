use super::*;

impl Sidebar {
    pub(super) fn start_renaming_thread(
        &mut self,
        ix: usize,
        thread_id: ThreadId,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.renaming_thread_id.is_some() && self.renaming_thread_id != Some(thread_id) {
            self.finish_thread_rename(window, cx);
        }

        self.selection = Some(ix);
        self.renaming_thread_id = Some(thread_id);
        self.suppress_next_rename_edit = true;
        self.list_state.scroll_to_reveal_item(ix);
        self.thread_rename_editor.update(cx, |editor, cx| {
            editor.set_text(title, window, cx);
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor.focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn handle_thread_rename_editor_event(
        &mut self,
        title_editor: &Entity<Editor>,
        event: &editor::EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            editor::EditorEvent::BufferEdited => {
                if self.suppress_next_rename_edit {
                    self.suppress_next_rename_edit = false;
                    return;
                }
                if !title_editor.read(cx).is_focused(window) {
                    return;
                }
                let new_title = title_editor.read(cx).text(cx);
                if new_title.is_empty() {
                    return;
                }
                let Some(thread_id) = self.renaming_thread_id else {
                    return;
                };
                self.apply_thread_rename(thread_id, SharedString::from(new_title), window, cx);
            }
            editor::EditorEvent::Blurred => {
                self.finish_thread_rename(window, cx);
            }
            _ => {}
        }
    }

    pub(super) fn apply_thread_rename(
        &mut self,
        thread_id: ThreadId,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut found = false;
        if let Some(multi_workspace) = self.multi_workspace.upgrade() {
            let workspaces: Vec<_> = multi_workspace.read(cx).workspaces().cloned().collect();
            for workspace in workspaces {
                let agent_thread_item = workspace
                    .read(cx)
                    .items_of_type::<AgentThreadItem>(cx)
                    .find(|item| item.read(cx).thread_id(cx) == thread_id);
                if let Some(agent_thread_item) = agent_thread_item
                    && let Some(thread_view) = agent_thread_item
                        .read(cx)
                        .conversation_view()
                        .read(cx)
                        .root_thread_view()
                {
                    thread_view.update(cx, |thread_view, cx| {
                        thread_view.rename(title.clone(), window, cx);
                    });
                    found = true;
                }

                if let Some(agent_panel) = workspace.read(cx).panel::<AgentPanel>(cx) {
                    if let Some(view) = agent_panel
                        .read(cx)
                        .conversation_view_for_id(&thread_id, cx)
                        && let Some(thread_view) = view.read(cx).root_thread_view()
                    {
                        thread_view.update(cx, |thread_view, cx| {
                            thread_view.rename(title.clone(), window, cx);
                        });
                        found = true;
                    }
                }
            }
        }

        if !found {
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.set_title_override(thread_id, title, cx);
            });
        }
    }

    pub(super) fn finish_thread_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.renaming_thread_id.take().is_none() {
            return false;
        }
        self.focus_handle.focus(window, cx);
        self.update_entries(cx);
        true
    }

    pub(super) fn editor_move_down(
        &mut self,
        _: &MoveDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_next(&SelectNext, window, cx);
        if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(super) fn editor_move_up(
        &mut self,
        _: &MoveUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_previous(&SelectPrevious, window, cx);
        if self.selection.is_some() {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self.selection {
            Some(ix) if ix + 1 < self.contents.entries.len() => ix + 1,
            Some(_) if !self.contents.entries.is_empty() => 0,
            None if !self.contents.entries.is_empty() => 0,
            _ => return,
        };
        self.selection = Some(next);
        self.list_state.scroll_to_reveal_item(next);
        cx.notify();
    }

    pub(super) fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.selection {
            Some(0) => {
                self.selection = None;
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            Some(ix) => {
                self.selection = Some(ix - 1);
                self.list_state.scroll_to_reveal_item(ix - 1);
                cx.notify();
            }
            None if !self.contents.entries.is_empty() => {
                let last = self.contents.entries.len() - 1;
                self.selection = Some(last);
                self.list_state.scroll_to_reveal_item(last);
                cx.notify();
            }
            None => {}
        }
    }

    pub(super) fn select_first(
        &mut self,
        _: &SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.contents.entries.is_empty() {
            self.selection = Some(0);
            self.list_state.scroll_to_reveal_item(0);
            cx.notify();
        }
    }

    pub(super) fn select_last(
        &mut self,
        _: &SelectLast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(last) = self.contents.entries.len().checked_sub(1) {
            self.selection = Some(last);
            self.list_state.scroll_to_reveal_item(last);
            cx.notify();
        }
    }

    pub(super) fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.finish_thread_rename(window, cx) {
            return;
        }

        let Some(ix) = self.selection else { return };
        let Some(entry) = self.contents.entries.get(ix) else {
            return;
        };

        match entry {
            ListEntry::ProjectHeader { key, .. } => {
                let key = key.clone();
                self.toggle_collapse(&key, window, cx);
            }
            ListEntry::Thread(thread) => {
                let metadata = thread.metadata.clone();
                match &thread.workspace {
                    ThreadEntryWorkspace::Open(workspace) => {
                        let workspace = workspace.clone();
                        self.activate_thread(metadata, &workspace, false, window, cx);
                    }
                    ThreadEntryWorkspace::Closed {
                        folder_paths,
                        project_group_key,
                    } => {
                        let folder_paths = folder_paths.clone();
                        let project_group_key = project_group_key.clone();
                        self.open_workspace_and_activate_thread(
                            metadata,
                            folder_paths,
                            &project_group_key,
                            window,
                            cx,
                        );
                    }
                }
            }
            ListEntry::Terminal(terminal) => {
                let metadata = terminal.metadata.clone();
                let workspace = terminal.workspace.clone();
                self.activate_terminal_entry(metadata, workspace, false, window, cx);
            }
        }
    }

    pub(super) fn find_workspace_across_windows(
        &self,
        cx: &App,
        predicate: impl Fn(&Entity<Workspace>, &App) -> bool,
    ) -> Option<(WindowHandle<MultiWorkspace>, Entity<Workspace>)> {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<MultiWorkspace>())
            .find_map(|window| {
                let workspace = window.read(cx).ok().and_then(|multi_workspace| {
                    multi_workspace
                        .workspaces()
                        .find(|workspace| predicate(workspace, cx))
                        .cloned()
                })?;
                Some((window, workspace))
            })
    }

    pub(super) fn find_workspace_in_current_window(
        &self,
        cx: &App,
        predicate: impl Fn(&Entity<Workspace>, &App) -> bool,
    ) -> Option<Entity<Workspace>> {
        self.multi_workspace.upgrade().and_then(|multi_workspace| {
            multi_workspace
                .read(cx)
                .workspaces()
                .find(|workspace| predicate(workspace, cx))
                .cloned()
        })
    }

    pub(super) fn load_agent_thread_in_workspace(
        workspace: &Entity<Workspace>,
        metadata: &ThreadMetadata,
        focus: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        open_agent_thread_in_workspace(workspace, metadata, focus, window, cx);
    }

    pub(super) fn open_closed_native_thread_as_markdown(
        session_id: &acp::SessionId,
        title: Option<SharedString>,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let thread_store = ThreadStore::global(cx);
        let load_task =
            thread_store.update(cx, |store, cx| store.load_thread(session_id.clone(), cx));

        let thread_title = title
            .map(|t| t.to_string())
            .unwrap_or_else(|| DEFAULT_THREAD_TITLE.to_string());

        let workspace = workspace.clone();

        window
            .spawn(cx, async move |cx| {
                let db_thread = load_task.await?;
                let Some(db_thread) = db_thread else {
                    anyhow::bail!("Thread not found in database");
                };

                let markdown = db_thread.to_markdown();

                cx.update(|window, cx| {
                    agent_ui::open_markdown_in_workspace(
                        thread_title,
                        markdown,
                        workspace,
                        window,
                        cx,
                    )
                })?
                .await
            })
            .detach_and_log_err(cx);
    }

    pub(super) fn show_thread_title_toast(
        workspace: Entity<Workspace>,
        message: &'static str,
        cx: &mut App,
    ) {
        workspace.update(cx, |workspace, cx| {
            let toast = StatusToast::new(message, cx, |this, _cx| {
                this.icon(
                    Icon::new(IconName::Warning)
                        .size(IconSize::Small)
                        .color(Color::Warning),
                )
                .dismiss_button(true)
            });
            workspace.toggle_status_toast(toast, cx);
        });
    }

    pub(super) fn show_no_thread_summary_model_toast(workspace: Entity<Workspace>, cx: &mut App) {
        Self::show_thread_title_toast(
            workspace,
            "No model is configured for summarizing thread titles.",
            cx,
        );
    }

    pub(super) fn regenerate_thread_title(
        &mut self,
        session_id: &acp::SessionId,
        thread_id: ThreadId,
        folder_paths: PathList,
        thread_workspace: Option<Entity<Workspace>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = thread_workspace
            .as_ref()
            .and_then(|w| w.read(cx).panel::<AgentPanel>(cx))
        {
            match panel.update(cx, |panel, cx| panel.regenerate_thread_title(thread_id, cx)) {
                ThreadTitleRegenerationResult::Started
                | ThreadTitleRegenerationResult::AlreadyGenerating => return,
                ThreadTitleRegenerationResult::NoModel => {
                    if let Some(workspace) = self.active_workspace(cx) {
                        Self::show_no_thread_summary_model_toast(workspace, cx);
                    }
                    return;
                }
                ThreadTitleRegenerationResult::NotOpen => {}
            }
        }

        let Some(configured_model) =
            LanguageModelRegistry::read_global(cx).thread_summary_model(cx)
        else {
            if let Some(workspace) = self.active_workspace(cx) {
                Self::show_no_thread_summary_model_toast(workspace, cx);
            }
            return;
        };

        if !self.regenerating_titles.insert(thread_id) {
            return;
        }

        let model = configured_model.model;
        let temperature = AgentSettings::temperature_for_model(&model, cx);

        let thread_store = ThreadStore::global(cx);
        let load_task =
            thread_store.update(cx, |store, cx| store.load_thread(session_id.clone(), cx));
        let session_id = session_id.clone();

        cx.notify();

        cx.spawn(async move |this, cx| {
            let result: anyhow::Result<SharedString> = async {
                let Some(db_thread) = load_task.await? else {
                    anyhow::bail!("Thread not found in database");
                };

                let request = agent::build_thread_title_request(&db_thread.messages, temperature);
                let title =
                    SharedString::from(agent::stream_thread_title(model, request, cx).await?);

                let Some(mut db_thread) = thread_store
                    .update(cx, |store, cx| store.load_thread(session_id.clone(), cx))
                    .await?
                else {
                    anyhow::bail!("Thread not found in database");
                };
                db_thread.title = title.clone();

                thread_store
                    .update(cx, |store, cx| {
                        store.save_thread(session_id, db_thread, folder_paths, cx)
                    })
                    .await?;

                anyhow::Ok(title)
            }
            .await;

            this.update(cx, |this, cx| {
                this.regenerating_titles.remove(&thread_id);
                match &result {
                    Ok(title) => {
                        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                            store.set_generated_title(thread_id, title.clone(), cx);
                        });
                    }
                    Err(_) => {
                        if let Some(workspace) = this.active_workspace(cx) {
                            Self::show_thread_title_toast(
                                workspace,
                                "Failed to regenerate thread title.",
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            })
            .ok();

            result.map(|_| ())
        })
        .detach_and_log_err(cx);
    }
}
