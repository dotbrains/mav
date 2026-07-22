use super::*;

impl CollabPanel {
    fn reset_filter_editor_text(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.filter_editor.update(cx, |editor, cx| {
            if editor.buffer().read(cx).len(cx).0 > 0 {
                editor.set_text("", window, cx);
                true
            } else {
                false
            }
        })
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if cx.stop_active_drag(window) {
            return;
        } else if self.take_editing_state(window, cx) {
            window.focus(&self.filter_editor.focus_handle(cx), cx);
        } else if !self.reset_filter_editor_text(window, cx) {
            self.focus_handle.focus(window, cx);
        }

        if self.context_menu.is_some() {
            self.context_menu.take();
            cx.notify();
        }

        self.update_entries(false, cx);
    }

    pub fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let ix = self.selection.map_or(0, |ix| ix + 1);
        if ix < self.entries.len() {
            self.selection = Some(ix);
        }

        if let Some(ix) = self.selection {
            self.scroll_to_item(ix)
        }
        cx.notify();
    }

    pub fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        let ix = self.selection.take().unwrap_or(0);
        if ix > 0 {
            self.selection = Some(ix - 1);
        }

        if let Some(ix) = self.selection {
            self.scroll_to_item(ix)
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirm_channel_edit(window, cx) {
            return;
        }

        if let Some(selection) = self.selection
            && let Some(entry) = self.entries.get(selection)
        {
            match entry {
                ListEntry::Header(section) => match section {
                    Section::ActiveCall => Self::leave_call(window, cx),
                    Section::Channels => self.new_root_channel(window, cx),
                    Section::Contacts => self.toggle_contact_finder(window, cx),
                    Section::FavoriteChannels
                    | Section::ContactRequests
                    | Section::Online
                    | Section::Offline
                    | Section::ChannelInvites => {
                        self.toggle_section_expanded(*section, cx);
                    }
                },
                ListEntry::Contact { contact, calling } => {
                    if contact.online && !contact.busy && !calling {
                        self.call(contact.user.legacy_id, window, cx);
                    }
                }
                ListEntry::ParticipantProject {
                    project_id,
                    host_user_id,
                    ..
                } => {
                    if let Some(workspace) = self.workspace.upgrade() {
                        let app_state = workspace.read(cx).app_state().clone();
                        workspace::join_in_room_project(*project_id, *host_user_id, app_state, cx)
                            .detach_and_prompt_err(
                                "Failed to join project",
                                window,
                                cx,
                                |error, _, _| Some(format!("{error:#}")),
                            );
                    }
                }
                ListEntry::ParticipantScreen { peer_id, .. } => {
                    let Some(peer_id) = peer_id else {
                        return;
                    };
                    if let Some(workspace) = self.workspace.upgrade() {
                        workspace.update(cx, |workspace, cx| {
                            workspace.open_shared_screen(*peer_id, window, cx)
                        });
                    }
                }
                ListEntry::Channel { channel, .. } => {
                    let is_active = maybe!({
                        let call_channel = ActiveCall::global(cx)
                            .read(cx)
                            .room()?
                            .read(cx)
                            .channel_id()?;

                        Some(call_channel == channel.id)
                    })
                    .unwrap_or(false);
                    if is_active {
                        self.open_channel_notes(channel.id, window, cx)
                    } else {
                        self.join_channel(channel.id, window, cx)
                    }
                }
                ListEntry::ContactPlaceholder => self.toggle_contact_finder(window, cx),
                ListEntry::CallParticipant { user, peer_id, .. } => {
                    if Some(user) == self.user_store.read(cx).current_user().as_ref() {
                        Self::leave_call(window, cx);
                    } else if let Some(peer_id) = peer_id {
                        self.workspace
                            .update(cx, |workspace, cx| workspace.follow(*peer_id, window, cx))
                            .ok();
                    }
                }
                ListEntry::IncomingRequest(user) => {
                    self.respond_to_contact_request(user.legacy_id, true, window, cx)
                }
                ListEntry::ChannelInvite(channel) => {
                    self.respond_to_channel_invite(channel.id, true, cx)
                }
                ListEntry::ChannelNotes { channel_id } => {
                    self.open_channel_notes(*channel_id, window, cx)
                }
                ListEntry::OutgoingRequest(_) => {}
                ListEntry::ChannelEditor { .. } => {}
            }
        }
    }

    fn insert_space(&mut self, _: &InsertSpace, window: &mut Window, cx: &mut Context<Self>) {
        if self.channel_editing_state.is_some() {
            self.channel_name_editor.update(cx, |editor, cx| {
                editor.insert(" ", window, cx);
            });
        } else if self.filter_editor.focus_handle(cx).is_focused(window) {
            self.filter_editor.update(cx, |editor, cx| {
                editor.insert(" ", window, cx);
            });
        }
    }
}
