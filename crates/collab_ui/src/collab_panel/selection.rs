use super::*;

impl CollabPanel {
    fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("CollabPanel");
        dispatch_context.add("menu");

        let identifier = if self.channel_name_editor.focus_handle(cx).is_focused(window)
            || self.filter_editor.focus_handle(cx).is_focused(window)
        {
            "editing"
        } else {
            "not_editing"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }

    fn selected_channel(&self) -> Option<&Arc<Channel>> {
        self.selection
            .and_then(|ix| self.entries.get(ix))
            .and_then(|entry| match entry {
                ListEntry::Channel { channel, .. } => Some(channel),
                _ => None,
            })
    }

    fn selected_entry_is_favorite(&self) -> bool {
        self.selection
            .and_then(|ix| self.entries.get(ix))
            .is_some_and(|entry| {
                matches!(
                    entry,
                    ListEntry::Channel {
                        is_favorite: true,
                        ..
                    }
                )
            })
    }

    fn selected_contact(&self) -> Option<Arc<Contact>> {
        self.selection
            .and_then(|ix| self.entries.get(ix))
            .and_then(|entry| match entry {
                ListEntry::Contact { contact, .. } => Some(contact.clone()),
                _ => None,
            })
    }

    fn show_channel_modal(
        &mut self,
        channel_id: ChannelId,
        mode: channel_modal::Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        let user_store = self.user_store.clone();
        let channel_store = self.channel_store.clone();

        cx.spawn_in(window, async move |_, cx| {
            workspace.update_in(cx, |workspace, window, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    ChannelModal::new(
                        user_store.clone(),
                        channel_store.clone(),
                        channel_id,
                        mode,
                        window,
                        cx,
                    )
                });
            })
        })
        .detach();
    }

    fn leave_channel(&self, channel_id: ChannelId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(user_id) = self.user_store.read(cx).current_user().map(|u| u.legacy_id) else {
            return;
        };
        let Some(channel) = self.channel_store.read(cx).channel_for_id(channel_id) else {
            return;
        };
        let prompt_message = format!("Are you sure you want to leave \"#{}\"?", channel.name);
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt_message,
            None,
            &["Leave", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await? != 0 {
                return Ok(());
            }
            this.update(cx, |this, cx| {
                this.channel_store.update(cx, |channel_store, cx| {
                    channel_store.remove_member(channel_id, user_id, cx)
                })
            })?
            .await
        })
        .detach_and_prompt_err("Failed to leave channel", window, cx, |_, _, _| None)
    }

    fn remove_channel(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let channel_store = self.channel_store.clone();
        if let Some(channel) = channel_store.read(cx).channel_for_id(channel_id) {
            let prompt_message = format!(
                "Are you sure you want to remove the channel \"{}\"?",
                channel.name
            );
            let answer = window.prompt(
                PromptLevel::Warning,
                &prompt_message,
                None,
                &["Remove", "Cancel"],
                cx,
            );
            let workspace = self.workspace.clone();
            cx.spawn_in(window, async move |this, mut cx| {
                if answer.await? == 0 {
                    channel_store
                        .update(cx, |channels, _| channels.remove_channel(channel_id))
                        .await
                        .notify_workspace_async_err(workspace, &mut cx);
                    this.update_in(cx, |_, window, cx| cx.focus_self(window))
                        .ok();
                }
                anyhow::Ok(())
            })
            .detach();
        }
    }

    fn respond_to_channel_invite(
        &mut self,
        channel_id: ChannelId,
        accept: bool,
        cx: &mut Context<Self>,
    ) {
        self.channel_store
            .update(cx, |store, cx| {
                store.respond_to_channel_invite(channel_id, accept, cx)
            })
            .detach();
    }

    fn call(&mut self, recipient_user_id: u64, window: &mut Window, cx: &mut Context<Self>) {
        ActiveCall::global(cx)
            .update(cx, |call, cx| {
                call.invite(recipient_user_id, Some(self.project.clone()), cx)
            })
            .detach_and_prompt_err("Call failed", window, cx, |_, _, _| None);
    }

    fn join_channel(&self, channel_id: ChannelId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let Some(handle) = window.window_handle().downcast::<MultiWorkspace>() else {
            return;
        };
        workspace::join_channel(
            channel_id,
            workspace.read(cx).app_state().clone(),
            Some(handle),
            Some(self.workspace.clone()),
            cx,
        )
        .detach_and_prompt_err("Failed to join channel", window, cx, |_, _, _| None)
    }

    fn copy_channel_link(&mut self, channel_id: ChannelId, cx: &mut Context<Self>) {
        let channel_store = self.channel_store.read(cx);
        let Some(channel) = channel_store.channel_for_id(channel_id) else {
            return;
        };
        let item = ClipboardItem::new_string(channel.link(cx));
        cx.write_to_clipboard(item)
    }

    fn copy_channel_notes_link(&mut self, channel_id: ChannelId, cx: &mut Context<Self>) {
        let channel_store = self.channel_store.read(cx);
        let Some(channel) = channel_store.channel_for_id(channel_id) else {
            return;
        };
        let item = ClipboardItem::new_string(channel.notes_link(None, cx));
        cx.write_to_clipboard(item)
    }
}
