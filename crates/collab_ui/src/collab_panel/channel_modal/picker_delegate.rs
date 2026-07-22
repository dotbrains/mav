use super::*;

impl PickerDelegate for ChannelModalDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "channel modal"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Search collaborator by username...".into()
    }

    fn match_count(&self) -> usize {
        match self.mode {
            Mode::ManageMembers => self.matching_member_indices.len(),
            Mode::InviteMembers => self.matching_users.len(),
        }
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        match self.mode {
            Mode::ManageMembers => {
                if self.has_all_members {
                    self.match_candidates.clear();
                    self.match_candidates.extend(
                        self.members.iter().enumerate().map(|(id, member)| {
                            StringMatchCandidate::new(id, &member.user.username)
                        }),
                    );

                    let matches = cx.foreground_executor().block_on(match_strings(
                        &self.match_candidates,
                        &query,
                        true,
                        true,
                        usize::MAX,
                        &Default::default(),
                        cx.background_executor().clone(),
                    ));

                    cx.spawn_in(window, async move |picker, cx| {
                        picker
                            .update(cx, |picker, cx| {
                                let delegate = &mut picker.delegate;
                                delegate.matching_member_indices.clear();
                                delegate
                                    .matching_member_indices
                                    .extend(matches.into_iter().map(|m| m.candidate_id));
                                cx.notify();
                            })
                            .ok();
                    })
                } else {
                    let search_members = self.channel_store.update(cx, |store, cx| {
                        store.fuzzy_search_members(self.channel_id, query.clone(), 100, cx)
                    });
                    cx.spawn_in(window, async move |picker, cx| {
                        async {
                            let members = search_members.await?;
                            picker.update(cx, |picker, cx| {
                                picker.delegate.has_all_members =
                                    query.is_empty() && members.len() < 100;
                                picker.delegate.matching_member_indices =
                                    (0..members.len()).collect();
                                picker.delegate.members = members;
                                cx.notify();
                            })?;
                            anyhow::Ok(())
                        }
                        .log_err()
                        .await;
                    })
                }
            }
            Mode::InviteMembers => {
                let search_users = self
                    .user_store
                    .update(cx, |store, cx| store.fuzzy_search_users(query, cx));
                cx.spawn_in(window, async move |picker, cx| {
                    async {
                        let users = search_users.await?;
                        picker.update(cx, |picker, cx| {
                            picker.delegate.matching_users = users;
                            cx.notify();
                        })?;
                        anyhow::Ok(())
                    }
                    .log_err()
                    .await;
                })
            }
        }
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(selected_user) = self.user_at_index(self.selected_index) {
            if Some(selected_user.legacy_id)
                == self
                    .user_store
                    .read(cx)
                    .current_user()
                    .map(|user| user.legacy_id)
            {
                return;
            }
            match self.mode {
                Mode::ManageMembers => self.show_context_menu(self.selected_index, window, cx),
                Mode::InviteMembers => match self.member_status(selected_user.legacy_id, cx) {
                    Some(proto::channel_member::Kind::Invitee) => {
                        self.remove_member(selected_user.legacy_id, window, cx);
                    }
                    Some(proto::channel_member::Kind::Member) => {}
                    None => self.invite_member(selected_user, window, cx),
                },
            }
        }
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        if self.context_menu.is_none() {
            self.channel_modal
                .update(cx, |_, cx| {
                    cx.emit(DismissEvent);
                })
                .ok();
        }
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let user = self.user_at_index(ix)?;
        let membership = self.member_at_index(ix);
        let request_status = self.member_status(user.legacy_id, cx);
        let is_me = self
            .user_store
            .read(cx)
            .current_user()
            .map(|user| user.legacy_id)
            == Some(user.legacy_id);

        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Avatar::new(user.avatar_uri.clone()))
                .child(Label::new(user.username.clone()))
                .end_slot(h_flex().gap_2().map(|slot| {
                    match self.mode {
                        Mode::ManageMembers => slot
                            .children(
                                if request_status == Some(proto::channel_member::Kind::Invitee) {
                                    Some(Label::new("Invited"))
                                } else {
                                    None
                                },
                            )
                            .children(match membership.map(|m| m.role) {
                                Some(ChannelRole::Admin) => Some(Label::new("Admin")),
                                Some(ChannelRole::Guest) => Some(Label::new("Guest")),
                                _ => None,
                            })
                            .when(!is_me, |el| {
                                el.child(IconButton::new("ellipsis", IconName::Ellipsis))
                            })
                            .when(is_me, |el| el.child(Label::new("You").color(Color::Muted)))
                            .children(
                                if let (Some((menu, _)), true) = (&self.context_menu, selected) {
                                    Some(
                                        deferred(
                                            anchored()
                                                .anchor(gpui::Anchor::TopRight)
                                                .child(menu.clone()),
                                        )
                                        .with_priority(1),
                                    )
                                } else {
                                    None
                                },
                            ),
                        Mode::InviteMembers => match request_status {
                            Some(proto::channel_member::Kind::Invitee) => {
                                slot.children(Some(Label::new("Invited")))
                            }
                            Some(proto::channel_member::Kind::Member) => {
                                slot.children(Some(Label::new("Member")))
                            }
                            _ => slot,
                        },
                    }
                })),
        )
    }
}
