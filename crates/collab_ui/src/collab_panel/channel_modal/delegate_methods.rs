use super::*;

impl ChannelModalDelegate {
    fn member_status(
        &self,
        user_id: LegacyUserId,
        cx: &App,
    ) -> Option<proto::channel_member::Kind> {
        self.members
            .iter()
            .find_map(|membership| {
                (membership.user.legacy_id == user_id).then_some(membership.kind)
            })
            .or_else(|| {
                self.channel_store
                    .read(cx)
                    .has_pending_channel_invite(self.channel_id, user_id)
                    .then_some(proto::channel_member::Kind::Invitee)
            })
    }

    fn member_at_index(&self, ix: usize) -> Option<&ChannelMembership> {
        self.matching_member_indices
            .get(ix)
            .and_then(|ix| self.members.get(*ix))
    }

    fn user_at_index(&self, ix: usize) -> Option<Arc<User>> {
        match self.mode {
            Mode::ManageMembers => self.matching_member_indices.get(ix).and_then(|ix| {
                let channel_membership = self.members.get(*ix)?;
                Some(channel_membership.user.clone())
            }),
            Mode::InviteMembers => self.matching_users.get(ix).cloned(),
        }
    }

    fn set_user_role(
        &mut self,
        user_id: LegacyUserId,
        new_role: ChannelRole,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<()> {
        let update = self.channel_store.update(cx, |store, cx| {
            store.set_member_role(self.channel_id, user_id, new_role, cx)
        });
        cx.spawn_in(window, async move |picker, cx| {
            update.await?;
            picker.update_in(cx, |picker, window, cx| {
                let this = &mut picker.delegate;
                if let Some(member) = this
                    .members
                    .iter_mut()
                    .find(|m| m.user.legacy_id == user_id)
                {
                    member.role = new_role;
                }
                cx.focus_self(window);
                cx.notify();
            })
        })
        .detach_and_prompt_err("Failed to update role", window, cx, |_, _, _| None);
        Some(())
    }

    fn remove_member(
        &mut self,
        user_id: LegacyUserId,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<()> {
        let update = self.channel_store.update(cx, |store, cx| {
            store.remove_member(self.channel_id, user_id, cx)
        });
        cx.spawn_in(window, async move |picker, cx| {
            update.await?;
            picker.update_in(cx, |picker, window, cx| {
                let this = &mut picker.delegate;
                if let Some(ix) = this
                    .members
                    .iter_mut()
                    .position(|m| m.user.legacy_id == user_id)
                {
                    this.members.remove(ix);
                    this.matching_member_indices.retain_mut(|member_ix| {
                        if *member_ix == ix {
                            return false;
                        } else if *member_ix > ix {
                            *member_ix -= 1;
                        }
                        true
                    })
                }

                this.selected_index = this
                    .selected_index
                    .min(this.matching_member_indices.len().saturating_sub(1));

                picker.focus(window, cx);
                cx.notify();
            })
        })
        .detach_and_prompt_err("Failed to remove member", window, cx, |_, _, _| None);
        Some(())
    }

    fn invite_member(
        &mut self,
        user: Arc<User>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let invite_member = self.channel_store.update(cx, |store, cx| {
            store.invite_member(self.channel_id, user.legacy_id, ChannelRole::Member, cx)
        });

        cx.spawn_in(window, async move |this, cx| {
            invite_member.await?;

            this.update(cx, |this, cx| {
                let new_member = ChannelMembership {
                    user,
                    kind: proto::channel_member::Kind::Invitee,
                    role: ChannelRole::Member,
                };
                let members = &mut this.delegate.members;
                match members.binary_search_by_key(&new_member.sort_key(), |k| k.sort_key()) {
                    Ok(ix) | Err(ix) => members.insert(ix, new_member),
                }

                cx.notify();
            })
        })
        .detach_and_prompt_err("Failed to invite member", window, cx, |_, _, _| None);
    }

    fn show_context_menu(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(membership) = self.member_at_index(ix) else {
            return;
        };
        let user_id = membership.user.legacy_id;
        let picker = cx.entity();
        let context_menu = ContextMenu::build(window, cx, |mut menu, _window, _cx| {
            let role = membership.role;

            if role == ChannelRole::Admin || role == ChannelRole::Member {
                let picker = picker.clone();
                menu = menu.entry("Demote to Guest", None, move |window, cx| {
                    picker.update(cx, |picker, cx| {
                        picker
                            .delegate
                            .set_user_role(user_id, ChannelRole::Guest, window, cx);
                    })
                });
            }

            if role == ChannelRole::Admin || role == ChannelRole::Guest {
                let picker = picker.clone();
                let label = if role == ChannelRole::Guest {
                    "Promote to Member"
                } else {
                    "Demote to Member"
                };

                menu = menu.entry(label, None, move |window, cx| {
                    picker.update(cx, |picker, cx| {
                        picker
                            .delegate
                            .set_user_role(user_id, ChannelRole::Member, window, cx);
                    })
                });
            }

            if role == ChannelRole::Member || role == ChannelRole::Guest {
                let picker = picker.clone();
                menu = menu.entry("Promote to Admin", None, move |window, cx| {
                    picker.update(cx, |picker, cx| {
                        picker
                            .delegate
                            .set_user_role(user_id, ChannelRole::Admin, window, cx);
                    })
                });
            };

            menu = menu.separator();
            menu = menu.entry("Remove from Channel", None, {
                let picker = picker.clone();
                move |window, cx| {
                    picker.update(cx, |picker, cx| {
                        picker.delegate.remove_member(user_id, window, cx);
                    })
                }
            });
            menu
        });
        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |picker, _, _: &DismissEvent, window, cx| {
                picker.delegate.context_menu = None;
                picker.focus(window, cx);
                cx.notify();
            },
        );
        self.context_menu = Some((context_menu, subscription));
    }
}
