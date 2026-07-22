use super::*;

impl CollabPanel {
    fn confirm_channel_edit(&mut self, window: &mut Window, cx: &mut Context<CollabPanel>) -> bool {
        if let Some(editing_state) = &mut self.channel_editing_state {
            match editing_state {
                ChannelEditingState::Create {
                    location,
                    pending_name,
                    ..
                } => {
                    if pending_name.is_some() {
                        return false;
                    }
                    let channel_name = self.channel_name_editor.read(cx).text(cx);

                    *pending_name = Some(channel_name.clone());

                    let create = self.channel_store.update(cx, |channel_store, cx| {
                        channel_store.create_channel(&channel_name, *location, cx)
                    });
                    if location.is_none() {
                        cx.spawn_in(window, async move |this, cx| {
                            let channel_id = create.await?;
                            this.update_in(cx, |this, window, cx| {
                                this.show_channel_modal(
                                    channel_id,
                                    channel_modal::Mode::InviteMembers,
                                    window,
                                    cx,
                                )
                            })
                        })
                        .detach_and_prompt_err(
                            "Failed to create channel",
                            window,
                            cx,
                            |_, _, _| None,
                        );
                    } else {
                        create.detach_and_prompt_err(
                            "Failed to create channel",
                            window,
                            cx,
                            |_, _, _| None,
                        );
                    }
                    cx.notify();
                }
                ChannelEditingState::Rename {
                    location,
                    pending_name,
                } => {
                    if pending_name.is_some() {
                        return false;
                    }
                    let channel_name = self.channel_name_editor.read(cx).text(cx);
                    *pending_name = Some(channel_name.clone());

                    self.channel_store
                        .update(cx, |channel_store, cx| {
                            channel_store.rename(*location, &channel_name, cx)
                        })
                        .detach();
                    cx.notify();
                }
            }
            cx.focus_self(window);
            true
        } else {
            false
        }
    }
}
