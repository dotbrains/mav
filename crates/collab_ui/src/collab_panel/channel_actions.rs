use super::*;

impl CollabPanel {
    fn leave_call(window: &mut Window, cx: &mut App) {
        ActiveCall::global(cx)
            .update(cx, |call, cx| call.hang_up(cx))
            .detach_and_prompt_err("Failed to hang up", window, cx, |_, _, _| None);
    }

    fn toggle_contact_finder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    let mut finder = ContactFinder::new(self.user_store.clone(), window, cx);
                    finder.set_query(self.filter_editor.read(cx).text(cx), window, cx);
                    finder
                });
            });
        }
    }

    fn new_root_channel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.channel_editing_state = Some(ChannelEditingState::Create {
            location: None,
            pending_name: None,
        });
        self.update_entries(false, cx);
        self.select_channel_editor();
        window.focus(&self.channel_name_editor.focus_handle(cx), cx);
        cx.notify();
    }

    fn select_channel_editor(&mut self) {
        self.selection = self
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::ChannelEditor { .. }));
    }

    fn new_subchannel(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.collapsed_channels
            .retain(|channel| *channel != channel_id);
        self.channel_editing_state = Some(ChannelEditingState::Create {
            location: Some(channel_id),
            pending_name: None,
        });
        self.update_entries(false, cx);
        self.select_channel_editor();
        window.focus(&self.channel_name_editor.focus_handle(cx), cx);
        cx.notify();
    }

    fn manage_members(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_channel_modal(channel_id, channel_modal::Mode::ManageMembers, window, cx);
    }

    fn remove_selected_channel(&mut self, _: &Remove, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(channel) = self.selected_channel() {
            self.remove_channel(channel.id, window, cx)
        }
    }

    fn rename_selected_channel(
        &mut self,
        _: &SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(channel) = self.selected_channel() {
            self.rename_channel(channel.id, window, cx);
        }
    }

    fn rename_channel(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let channel_store = self.channel_store.read(cx);
        if !channel_store.is_channel_admin(channel_id) {
            return;
        }
        if let Some(channel) = channel_store.channel_for_id(channel_id).cloned() {
            self.channel_editing_state = Some(ChannelEditingState::Rename {
                location: channel_id,
                pending_name: None,
            });
            self.channel_name_editor.update(cx, |editor, cx| {
                editor.set_text(channel.name.clone(), window, cx);
                editor.select_all(&Default::default(), window, cx);
            });
            window.focus(&self.channel_name_editor.focus_handle(cx), cx);
            self.update_entries(false, cx);
            self.select_channel_editor();
        }
    }

    fn open_selected_channel_notes(
        &mut self,
        _: &OpenSelectedChannelNotes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(channel) = self.selected_channel() {
            self.open_channel_notes(channel.id, window, cx);
        }
    }

    pub fn toggle_selected_channel_favorite(
        &mut self,
        _: &ToggleSelectedChannelFavorite,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(channel) = self.selected_channel() {
            self.toggle_favorite_channel(channel.id, cx);
        }
    }

    fn set_channel_visibility(
        &mut self,
        channel_id: ChannelId,
        visibility: ChannelVisibility,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.channel_store
            .update(cx, |channel_store, cx| {
                channel_store.set_channel_visibility(channel_id, visibility, cx)
            })
            .detach_and_prompt_err("Failed to set channel visibility", window, cx, |e, _, _| match e.error_code() {
                ErrorCode::BadPublicNesting =>
                    if e.error_tag("direction") == Some("parent") {
                        Some("To make a channel public, its parent channel must be public.".to_string())
                    } else {
                        Some("To make a channel private, all of its subchannels must be private.".to_string())
                    },
                _ => None
            });
    }

    fn start_move_channel(
        &mut self,
        channel_id: ChannelId,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.channel_clipboard = Some(ChannelMoveClipboard { channel_id });
    }

    fn start_move_selected_channel(
        &mut self,
        _: &StartMoveChannel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(channel) = self.selected_channel() {
            self.start_move_channel(channel.id, window, cx);
        }
    }

    fn move_channel_on_clipboard(
        &mut self,
        to_channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<CollabPanel>,
    ) {
        if let Some(clipboard) = self.channel_clipboard.take() {
            self.move_channel(clipboard.channel_id, to_channel_id, window, cx)
        }
    }

    fn move_channel(
        &self,
        channel_id: ChannelId,
        to: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.channel_store
            .update(cx, |channel_store, cx| {
                channel_store.move_channel(channel_id, to, cx)
            })
            .detach_and_prompt_err("Failed to move channel", window, cx, |e, _, _| {
                match e.error_code() {
                    ErrorCode::BadPublicNesting => {
                        Some("Public channels must have public parents".into())
                    }
                    ErrorCode::CircularNesting => {
                        Some("You cannot move a channel into itself".into())
                    }
                    ErrorCode::WrongMoveTarget => {
                        Some("You cannot move a channel into a different root channel".into())
                    }
                    _ => None,
                }
            })
    }

    pub fn move_channel_up(
        &mut self,
        _: &MoveChannelUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reorder_selected_channel(Direction::Up, window, cx);
    }

    pub fn move_channel_down(
        &mut self,
        _: &MoveChannelDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reorder_selected_channel(Direction::Down, window, cx);
    }

    fn reorder_selected_channel(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(channel) = self.selected_channel().cloned() {
            if self.selected_entry_is_favorite() {
                self.reorder_favorite(channel.id, direction, cx);
                return;
            }

            self.channel_store.update(cx, |store, cx| {
                store
                    .reorder_channel(channel.id, direction, cx)
                    .detach_and_prompt_err(
                        match direction {
                            Direction::Up => "Failed to move channel up",
                            Direction::Down => "Failed to move channel down",
                        },
                        window,
                        cx,
                        |_, _, _| None,
                    )
            });
        }
    }

    pub fn reorder_favorite(
        &mut self,
        channel_id: ChannelId,
        direction: Direction,
        cx: &mut Context<Self>,
    ) {
        self.channel_store.update(cx, |store, cx| {
            let favorite_ids = store.favorite_channel_ids();
            let Some(channel_index) = favorite_ids.iter().position(|id| *id == channel_id) else {
                return;
            };
            let target_channel_index = match direction {
                Direction::Up => channel_index.checked_sub(1),
                Direction::Down => {
                    let next = channel_index + 1;
                    (next < favorite_ids.len()).then_some(next)
                }
            };
            if let Some(target_channel_index) = target_channel_index {
                let mut new_ids = favorite_ids.to_vec();
                new_ids.swap(channel_index, target_channel_index);
                store.set_favorite_channel_ids(new_ids, cx);
            }
        });
        self.persist_favorites(cx);
    }

    fn open_channel_notes(
        &mut self,
        channel_id: ChannelId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            ChannelView::open(channel_id, None, workspace, window, cx).detach();
        }
    }

    fn show_inline_context_menu(
        &mut self,
        _: &Secondary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self
            .selection
            .and_then(|ix| self.list_state.bounds_for_item(ix))
        else {
            return;
        };

        if let Some(channel) = self.selected_channel() {
            self.deploy_channel_context_menu(
                bounds.center(),
                channel.id,
                self.selection.unwrap(),
                window,
                cx,
            );
            cx.stop_propagation();
            return;
        };

        if let Some(contact) = self.selected_contact() {
            self.deploy_contact_context_menu(bounds.center(), contact, window, cx);
            cx.stop_propagation();
        }
    }
}
