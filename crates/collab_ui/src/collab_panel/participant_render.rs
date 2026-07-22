use super::*;

impl CollabPanel {
    fn render_call_participant(
        &self,
        user: &Arc<User>,
        peer_id: Option<PeerId>,
        is_pending: bool,
        role: proto::ChannelRole,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> ListItem {
        let user_id = user.legacy_id;
        let is_current_user = self
            .user_store
            .read(cx)
            .current_user()
            .map(|user| user.legacy_id)
            == Some(user_id);
        let tooltip = format!("Follow {}", user.username);

        let is_call_admin = ActiveCall::global(cx).read(cx).room().is_some_and(|room| {
            room.read(cx).local_participant().role == proto::ChannelRole::Admin
        });

        let end_slot = if is_pending {
            Label::new("Calling").color(Color::Muted).into_any_element()
        } else if is_current_user {
            IconButton::new("leave-call", IconName::Exit)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text("Leave Call"))
                .on_click(move |_, window, cx| Self::leave_call(window, cx))
                .into_any_element()
        } else if role == proto::ChannelRole::Guest {
            Label::new("Guest").color(Color::Muted).into_any_element()
        } else if role == proto::ChannelRole::Talker {
            Label::new("Mic only")
                .color(Color::Muted)
                .into_any_element()
        } else {
            Empty.into_any_element()
        };

        ListItem::new(user.username.clone())
            .start_slot(Avatar::new(user.avatar_uri.clone()))
            .child(render_participant_name_and_handle(user))
            .toggle_state(is_selected)
            .end_slot(end_slot)
            .tooltip(Tooltip::text("Click to Follow"))
            .when_some(peer_id, |el, peer_id| {
                if role == proto::ChannelRole::Guest {
                    return el;
                }
                el.tooltip(Tooltip::text(tooltip.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.workspace
                            .update(cx, |workspace, cx| workspace.follow(peer_id, window, cx))
                            .ok();
                    }))
            })
            .when(is_call_admin, |el| {
                el.on_secondary_mouse_down(cx.listener(
                    move |this, event: &MouseDownEvent, window, cx| {
                        this.deploy_participant_context_menu(
                            event.position,
                            user_id,
                            role,
                            window,
                            cx,
                        )
                    },
                ))
            })
    }

    fn render_participant_project(
        &self,
        project_id: u64,
        worktree_root_names: &[String],
        host_user_id: u64,
        is_last: bool,
        is_selected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let project_name: SharedString = if worktree_root_names.is_empty() {
            "untitled".to_string()
        } else {
            worktree_root_names.join(", ")
        }
        .into();

        ListItem::new(project_id as usize)
            .height(rems_from_px(24.))
            .toggle_state(is_selected)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.workspace
                    .update(cx, |workspace, cx| {
                        let app_state = workspace.app_state().clone();
                        workspace::join_in_room_project(project_id, host_user_id, app_state, cx)
                            .detach_and_prompt_err(
                                "Failed to join project",
                                window,
                                cx,
                                |error, _, _| Some(format!("{error:#}")),
                            );
                    })
                    .ok();
            }))
            .start_slot(
                h_flex()
                    .gap_1p5()
                    .child(render_tree_branch(is_last, false, window, cx))
                    .child(
                        Icon::new(IconName::Folder)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(Label::new(project_name.clone()))
            .tooltip(Tooltip::text(format!("Open {}", project_name)))
    }

    fn render_participant_screen(
        &self,
        peer_id: Option<PeerId>,
        is_last: bool,
        is_selected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = peer_id.map_or(usize::MAX, |id| id.as_u64() as usize);

        ListItem::new(("screen", id))
            .height(rems_from_px(24.))
            .toggle_state(is_selected)
            .start_slot(
                h_flex()
                    .gap_1p5()
                    .child(render_tree_branch(is_last, false, window, cx))
                    .child(
                        Icon::new(IconName::Screen)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(Label::new("Screen"))
            .when_some(peer_id, |this, _| {
                this.on_click(cx.listener(move |this, _, window, cx| {
                    this.workspace
                        .update(cx, |workspace, cx| {
                            workspace.open_shared_screen(peer_id.unwrap(), window, cx)
                        })
                        .ok();
                }))
                .tooltip(Tooltip::text("Open Shared Screen"))
            })
    }

    fn take_editing_state(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.channel_editing_state.take().is_some() {
            self.channel_name_editor.update(cx, |editor, cx| {
                editor.set_text("", window, cx);
            });
            true
        } else {
            false
        }
    }

    fn render_channel_notes(
        &self,
        channel_id: ChannelId,
        is_selected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let channel_store = self.channel_store.read(cx);
        let has_channel_buffer_changed = channel_store.has_channel_buffer_changed(channel_id);

        ListItem::new("channel-notes")
            .height(rems_from_px(24.))
            .toggle_state(is_selected)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_channel_notes(channel_id, window, cx);
            }))
            .start_slot(
                h_flex()
                    .relative()
                    .gap_1p5()
                    .child(render_tree_branch(false, true, window, cx))
                    .child(
                        h_flex()
                            .child(
                                Icon::new(IconName::Reader)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .when(has_channel_buffer_changed, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .top_neg_0p5()
                                        .right_0()
                                        .child(Indicator::dot().color(Color::Info)),
                                )
                            }),
                    ),
            )
            .child(Label::new("notes"))
            .tooltip(Tooltip::text("Open Channel Notes"))
    }
}
