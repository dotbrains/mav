use super::*;

impl CollabPanel {
    fn render_disabled_by_organization(&mut self, _cx: &mut Context<Self>) -> Div {
        v_flex()
            .p_4()
            .gap_4()
            .size_full()
            .text_center()
            .justify_center()
            .child(Label::new(
                "Collaboration is disabled for this organization.",
            ))
    }

    fn render_signed_out(&mut self, cx: &mut Context<Self>) -> Div {
        let collab_blurb = "Work with your team in realtime with collaborative editing, voice, shared notes and more.";

        // Two distinct "not connected" states:
        //   - Authenticated (has credentials): user just needs to connect.
        //   - Unauthenticated (no credentials): user needs to sign in via GitHub.
        let is_authenticated = self.client.user_id().is_some();
        let status = *self.client.status().borrow();
        let is_busy = status.is_signing_in();

        let (button_id, button_label, button_icon) = if is_authenticated {
            (
                "connect",
                if is_busy { "Connecting…" } else { "Connect" },
                IconName::Public,
            )
        } else {
            (
                "sign_in",
                if is_busy {
                    "Signing in…"
                } else {
                    "Sign In with GitHub"
                },
                IconName::Github,
            )
        };

        v_flex()
            .p_4()
            .gap_4()
            .size_full()
            .text_center()
            .justify_center()
            .child(Label::new(collab_blurb))
            .child(
                Button::new(button_id, button_label)
                    .full_width()
                    .start_icon(Icon::new(button_icon).color(Color::Muted))
                    .style(ButtonStyle::Outlined)
                    .disabled(is_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let client = this.client.clone();
                        let workspace = this.workspace.clone();
                        cx.spawn_in(window, async move |_, mut cx| {
                            client
                                .connect(true, &mut cx)
                                .await
                                .into_response()
                                .notify_workspace_async_err(workspace, &mut cx);
                        })
                        .detach()
                    })),
            )
    }

    fn render_list_entry(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = self.entries[ix].clone();

        let is_selected = self.selection == Some(ix);
        match entry {
            ListEntry::Header(section) => {
                let is_collapsed = self.collapsed_sections.contains(&section);
                self.render_header(section, is_selected, is_collapsed, cx)
                    .into_any_element()
            }
            ListEntry::Contact { contact, calling } => {
                self.mark_contact_request_accepted_notifications_read(contact.user.legacy_id, cx);
                self.render_contact(&contact, calling, is_selected, cx)
                    .into_any_element()
            }
            ListEntry::ContactPlaceholder => self
                .render_contact_placeholder(is_selected, cx)
                .into_any_element(),
            ListEntry::IncomingRequest(user) => self
                .render_contact_request(&user, true, is_selected, cx)
                .into_any_element(),
            ListEntry::OutgoingRequest(user) => self
                .render_contact_request(&user, false, is_selected, cx)
                .into_any_element(),
            ListEntry::Channel {
                channel,
                depth,
                has_children,
                string_match,
                ..
            } => self
                .render_channel(
                    &channel,
                    depth,
                    has_children,
                    is_selected,
                    ix,
                    string_match.as_ref(),
                    cx,
                )
                .into_any_element(),
            ListEntry::ChannelEditor { depth } => self
                .render_channel_editor(depth, window, cx)
                .into_any_element(),
            ListEntry::ChannelInvite(channel) => self
                .render_channel_invite(&channel, is_selected, cx)
                .into_any_element(),
            ListEntry::CallParticipant {
                user,
                peer_id,
                is_pending,
                role,
            } => self
                .render_call_participant(&user, peer_id, is_pending, role, is_selected, cx)
                .into_any_element(),
            ListEntry::ParticipantProject {
                project_id,
                worktree_root_names,
                host_user_id,
                is_last,
            } => self
                .render_participant_project(
                    project_id,
                    &worktree_root_names,
                    host_user_id,
                    is_last,
                    is_selected,
                    window,
                    cx,
                )
                .into_any_element(),
            ListEntry::ParticipantScreen { peer_id, is_last } => self
                .render_participant_screen(peer_id, is_last, is_selected, window, cx)
                .into_any_element(),
            ListEntry::ChannelNotes { channel_id } => self
                .render_channel_notes(channel_id, is_selected, window, cx)
                .into_any_element(),
        }
    }

    fn render_signed_in(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        self.channel_store.update(cx, |channel_store, _| {
            channel_store.initialize();
        });

        let has_query = !self.filter_editor.read(cx).text(cx).is_empty();

        v_flex()
            .size_full()
            .gap_1()
            .child(
                h_flex()
                    .p_2()
                    .h(Tab::container_height(cx))
                    .gap_1p5()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(self.render_filter_input(&self.filter_editor, cx))
                    .when(has_query, |this| {
                        this.pr_2p5().child(
                            IconButton::new("clear_filter", IconName::Close)
                                .shape(IconButtonShape::Square)
                                .tooltip(Tooltip::text("Clear Filter"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.reset_filter_editor_text(window, cx);
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(Self::render_list_entry),
                )
                .size_full(),
            )
    }
}
