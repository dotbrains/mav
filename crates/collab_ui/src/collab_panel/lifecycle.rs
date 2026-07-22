use super::*;

impl CollabPanel {
    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let filter_editor = cx.new(|cx| {
                let mut editor = Editor::single_line(window, cx);
                editor.set_placeholder_text("Search channels…", window, cx);
                editor
            });

            cx.subscribe(&filter_editor, |this: &mut Self, _, event, cx| {
                if let editor::EditorEvent::BufferEdited = event {
                    let query = this.filter_editor.read(cx).text(cx);
                    if !query.is_empty() {
                        this.selection.take();
                    }
                    this.update_entries(true, cx);
                    if !query.is_empty() {
                        this.selection = this
                            .entries
                            .iter()
                            .position(|entry| !matches!(entry, ListEntry::Header(_)));
                    }
                }
            })
            .detach();

            let channel_name_editor = cx.new(|cx| Editor::single_line(window, cx));

            cx.subscribe_in(
                &channel_name_editor,
                window,
                |this: &mut Self, _, event, window, cx| {
                    if let editor::EditorEvent::Blurred = event {
                        if let Some(state) = &this.channel_editing_state
                            && state.pending_name().is_some()
                        {
                            return;
                        }
                        this.take_editing_state(window, cx);
                        this.update_entries(false, cx);
                        cx.notify();
                    }
                },
            )
            .detach();

            let mut this = Self {
                focus_handle: cx.focus_handle(),
                channel_clipboard: None,
                fs: workspace.app_state().fs.clone(),
                pending_panel_serialization: Task::ready(None),
                pending_favorites_serialization: Task::ready(None),
                pending_filter_serialization: Task::ready(None),
                context_menu: None,
                list_state: ListState::new(0, gpui::ListAlignment::Top, px(1000.)),
                channel_name_editor,
                filter_editor,
                entries: Vec::default(),
                channel_editing_state: None,
                selection: None,
                channel_store: ChannelStore::global(cx),
                notification_store: NotificationStore::global(cx),
                current_notification_toast: None,
                mark_as_read_tasks: HashMap::default(),
                user_store: workspace.user_store().clone(),
                project: workspace.project().clone(),
                subscriptions: Vec::default(),
                match_candidates: Vec::default(),
                collapsed_sections: vec![Section::Offline],
                collapsed_channels: Vec::default(),
                filter_occupied_channels: false,
                workspace: workspace.weak_handle(),
                client: workspace.app_state().client.clone(),
            };

            this.update_entries(false, cx);

            let active_call = ActiveCall::global(cx);
            this.subscriptions
                .push(cx.observe(&this.user_store, |this, _, cx| {
                    this.update_entries(true, cx)
                }));
            this.subscriptions
                .push(cx.observe(&this.channel_store, move |this, _, cx| {
                    this.update_entries(true, cx)
                }));
            this.subscriptions
                .push(cx.observe(&active_call, |this, _, cx| this.update_entries(true, cx)));
            this.subscriptions.push(cx.subscribe_in(
                &this.channel_store,
                window,
                |this, _channel_store, e, window, cx| match e {
                    ChannelEvent::ChannelCreated(channel_id)
                    | ChannelEvent::ChannelRenamed(channel_id) => {
                        if this.take_editing_state(window, cx) {
                            this.update_entries(false, cx);
                            this.selection = this.entries.iter().position(|entry| {
                                if let ListEntry::Channel { channel, .. } = entry {
                                    channel.id == *channel_id
                                } else {
                                    false
                                }
                            });
                        }
                    }
                },
            ));
            this.subscriptions.push(cx.subscribe_in(
                &this.notification_store,
                window,
                Self::on_notification_event,
            ));

            this
        })
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let serialized_panel = match workspace
            .read_with(&cx, |workspace, _| {
                CollabPanel::serialization_key(workspace)
            })
            .ok()
            .flatten()
        {
            Some(serialization_key) => {
                let kvp = cx.update(|_, cx| KeyValueStore::global(cx))?;
                kvp.read_kvp(&serialization_key)
                    .context("reading collaboration panel from key value store")
                    .log_err()
                    .flatten()
                    .map(|panel| serde_json::from_str::<SerializedCollabPanel>(&panel))
                    .transpose()
                    .log_err()
                    .flatten()
            }
            None => None,
        };

        workspace.update_in(&mut cx, |workspace, window, cx| {
            let panel = CollabPanel::new(workspace, window, cx);
            if let Some(serialized_panel) = serialized_panel {
                panel.update(cx, |panel, cx| {
                    panel.collapsed_channels = serialized_panel
                        .collapsed_channels
                        .unwrap_or_default()
                        .iter()
                        .map(|cid| ChannelId(*cid))
                        .collect();
                    cx.notify();
                });
            }

            let filter_occupied_channels = KeyValueStore::global(cx)
                .read_kvp(FILTER_OCCUPIED_CHANNELS_KEY)
                .ok()
                .flatten()
                .is_some();

            panel.update(cx, |panel, cx| {
                panel.filter_occupied_channels = filter_occupied_channels;

                if filter_occupied_channels {
                    panel.update_entries(false, cx);
                }
            });

            let favorites: Vec<ChannelId> = KeyValueStore::global(cx)
                .read_kvp(FAVORITE_CHANNELS_KEY)
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str::<Vec<u64>>(&json).ok())
                .unwrap_or_default()
                .into_iter()
                .map(ChannelId)
                .collect();

            if !favorites.is_empty() {
                panel.update(cx, |panel, cx| {
                    panel.channel_store.update(cx, |store, cx| {
                        store.set_favorite_channel_ids(favorites, cx);
                    });
                });
            }

            panel
        })
    }

    fn serialization_key(workspace: &Workspace) -> Option<String> {
        workspace
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or(workspace.session_id())
            .map(|id| format!("{}-{:?}", COLLABORATION_PANEL_KEY, id))
    }

    fn serialize(&mut self, cx: &mut Context<Self>) {
        let Some(serialization_key) = self
            .workspace
            .read_with(cx, |workspace, _| CollabPanel::serialization_key(workspace))
            .ok()
            .flatten()
        else {
            return;
        };
        let collapsed_channels = if self.collapsed_channels.is_empty() {
            None
        } else {
            Some(self.collapsed_channels.iter().map(|id| id.0).collect())
        };

        let kvp = KeyValueStore::global(cx);
        self.pending_panel_serialization = cx.background_spawn(
            async move {
                kvp.write_kvp(
                    serialization_key,
                    serde_json::to_string(&SerializedCollabPanel { collapsed_channels })?,
                )
                .await?;
                anyhow::Ok(())
            }
            .log_err(),
        );
    }

    fn scroll_to_item(&mut self, ix: usize) {
        self.list_state.scroll_to_reveal_item(ix)
    }
}
