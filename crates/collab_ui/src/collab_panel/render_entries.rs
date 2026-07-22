use super::*;

impl CollabPanel {
    fn render_channel_invite(
        &self,
        channel: &Arc<Channel>,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> ListItem {
        let channel_id = channel.id;
        let response_is_pending = self
            .channel_store
            .read(cx)
            .has_pending_channel_invite_response(channel);
        let color = if response_is_pending {
            Color::Muted
        } else {
            Color::Default
        };

        let controls = [
            IconButton::new("reject-invite", IconName::Close)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.respond_to_channel_invite(channel_id, false, cx);
                }))
                .icon_color(color)
                .tooltip(Tooltip::text("Decline invite")),
            IconButton::new("accept-invite", IconName::Check)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.respond_to_channel_invite(channel_id, true, cx);
                }))
                .icon_color(color)
                .tooltip(Tooltip::text("Accept invite")),
        ];

        ListItem::new(("channel-invite", channel.id.0 as usize))
            .toggle_state(is_selected)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(Label::new(channel.name.clone()))
                    .child(h_flex().children(controls)),
            )
            .start_slot(
                Icon::new(IconName::Hash)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
    }

    fn render_channel(
        &self,
        channel: &Channel,
        depth: usize,
        has_children: bool,
        is_selected: bool,
        ix: usize,
        string_match: Option<&StringMatch>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let channel_id = channel.id;

        let is_active = maybe!({
            let call_channel = ActiveCall::global(cx)
                .read(cx)
                .room()?
                .read(cx)
                .channel_id()?;
            Some(call_channel == channel_id)
        })
        .unwrap_or(false);
        let channel_store = self.channel_store.read(cx);
        let is_public = channel_store
            .channel_for_id(channel_id)
            .map(|channel| channel.visibility)
            == Some(proto::ChannelVisibility::Public);
        let disclosed =
            has_children.then(|| self.collapsed_channels.binary_search(&channel.id).is_err());

        let has_notes_notification = channel_store.has_channel_buffer_changed(channel_id);

        const FACEPILE_LIMIT: usize = 3;
        let participants = self.channel_store.read(cx).channel_participants(channel_id);

        let face_pile = if participants.is_empty() {
            None
        } else {
            let extra_count = participants.len().saturating_sub(FACEPILE_LIMIT);
            let result = Facepile::new(
                participants
                    .iter()
                    .map(|user| Avatar::new(user.avatar_uri.clone()).into_any_element())
                    .take(FACEPILE_LIMIT)
                    .chain(if extra_count > 0 {
                        Some(
                            Label::new(format!("+{extra_count}"))
                                .ml_2()
                                .into_any_element(),
                        )
                    } else {
                        None
                    })
                    .collect::<SmallVec<_>>(),
            );

            Some(result)
        };

        let width = self
            .workspace
            .read_with(cx, |workspace, cx| {
                workspace
                    .panel_size_state::<Self>(cx)
                    .and_then(|size_state| size_state.size)
            })
            .ok()
            .flatten()
            .unwrap_or(px(240.));
        let root_id = channel.root_id();

        let is_favorited = self.is_channel_favorited(channel_id, cx);
        let (favorite_icon, favorite_color, favorite_tooltip) = if is_favorited {
            (IconName::StarFilled, Color::Accent, "Remove from Favorites")
        } else {
            (IconName::Star, Color::Default, "Add to Favorites")
        };

        let height = rems_from_px(24.);

        h_flex()
            .id(ix)
            .group("")
            .h(height)
            .w_full()
            .overflow_hidden()
            .when(!channel.is_root_channel(), |el| {
                el.on_drag(channel.clone(), move |channel, _, _, cx| {
                    cx.new(|_| DraggedChannelView {
                        channel: channel.clone(),
                        width,
                    })
                })
            })
            .drag_over::<Channel>({
                move |style, dragged_channel: &Channel, _window, cx| {
                    if dragged_channel.root_id() == root_id {
                        style.bg(cx.theme().colors().ghost_element_hover)
                    } else {
                        style
                    }
                }
            })
            .on_drop(
                cx.listener(move |this, dragged_channel: &Channel, window, cx| {
                    if dragged_channel.root_id() != root_id {
                        return;
                    }
                    this.move_channel(dragged_channel.id, channel_id, window, cx);
                }),
            )
            .child(
                ListItem::new(ix)
                    .height(height)
                    // Add one level of depth for the disclosure arrow.
                    .indent_level(depth + 1)
                    .indent_step_size(px(20.))
                    .toggle_state(is_selected || is_active)
                    .toggle(disclosed)
                    .on_toggle(cx.listener(move |this, _, window, cx| {
                        this.toggle_channel_collapsed(channel_id, window, cx)
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if is_active {
                            this.open_channel_notes(channel_id, window, cx)
                        } else {
                            this.join_channel(channel_id, window, cx)
                        }
                    }))
                    .on_secondary_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, window, cx| {
                            this.deploy_channel_context_menu(
                                event.position,
                                channel_id,
                                ix,
                                window,
                                cx,
                            )
                        },
                    ))
                    .child(
                        h_flex()
                            .id(format!("inside-{}", channel_id.0))
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .relative()
                                    .child(
                                        Icon::new(if is_public {
                                            IconName::Public
                                        } else {
                                            IconName::Hash
                                        })
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                    )
                                    .children(has_notes_notification.then(|| {
                                        div()
                                            .w_1p5()
                                            .absolute()
                                            .right(px(-1.))
                                            .top(px(-1.))
                                            .child(Indicator::dot().color(Color::Info))
                                    })),
                            )
                            .child(
                                h_flex()
                                    .id(channel_id.0 as usize)
                                    .child(match string_match {
                                        None => Label::new(channel.name.clone()).into_any_element(),
                                        Some(string_match) => HighlightedLabel::new(
                                            channel.name.clone(),
                                            string_match.positions.clone(),
                                        )
                                        .into_any_element(),
                                    })
                                    .children(face_pile.map(|face_pile| face_pile.p_1())),
                            )
                            .tooltip({
                                let channel_store = self.channel_store.clone();
                                move |_window, cx| {
                                    cx.new(|_| JoinChannelTooltip {
                                        channel_store: channel_store.clone(),
                                        channel_id,
                                        has_notes_notification,
                                    })
                                    .into()
                                }
                            }),
                    ),
            )
            .child(
                h_flex()
                    .visible_on_hover("")
                    .h_full()
                    .absolute()
                    .right_0()
                    .px_1()
                    .gap_px()
                    .rounded_l_md()
                    .bg(cx.theme().colors().background)
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("channel_favorite", favorite_icon)
                            .icon_size(IconSize::Small)
                            .icon_color(favorite_color)
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.toggle_favorite_channel(channel_id, cx)
                            }))
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action_in(
                                    favorite_tooltip,
                                    &ToggleSelectedChannelFavorite,
                                    &focus_handle,
                                    cx,
                                )
                            })
                    })
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("channel_notes", IconName::Reader)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_channel_notes(channel_id, window, cx)
                            }))
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action_in(
                                    "Open Channel Notes",
                                    &OpenSelectedChannelNotes,
                                    &focus_handle,
                                    cx,
                                )
                            })
                    }),
            )
    }

    fn render_channel_editor(
        &self,
        depth: usize,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let item = ListItem::new("channel-editor")
            .inset(false)
            // Add one level of depth for the disclosure arrow.
            .indent_level(depth + 1)
            .indent_step_size(px(20.))
            .start_slot(
                Icon::new(IconName::Hash)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            );

        if let Some(pending_name) = self
            .channel_editing_state
            .as_ref()
            .and_then(|state| state.pending_name())
        {
            item.child(Label::new(pending_name))
        } else {
            item.child(self.channel_name_editor.clone())
        }
    }
}
