use super::*;

impl CollabPanel {
    fn has_subchannels(&self, ix: usize) -> bool {
        self.entries.get(ix).is_some_and(|entry| {
            if let ListEntry::Channel { has_children, .. } = entry {
                *has_children
            } else {
                false
            }
        })
    }

    fn deploy_participant_context_menu(
        &mut self,
        position: Point<Pixels>,
        user_id: u64,
        role: proto::ChannelRole,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity();
        if !(role == proto::ChannelRole::Guest
            || role == proto::ChannelRole::Talker
            || role == proto::ChannelRole::Member)
        {
            return;
        }

        let context_menu = ContextMenu::build(window, cx, |mut context_menu, window, _| {
            if role == proto::ChannelRole::Guest {
                context_menu = context_menu.entry(
                    "Grant Mic Access",
                    None,
                    window.handler_for(&this, move |_, window, cx| {
                        ActiveCall::global(cx)
                            .update(cx, |call, cx| {
                                let Some(room) = call.room() else {
                                    return Task::ready(Ok(()));
                                };
                                room.update(cx, |room, cx| {
                                    room.set_participant_role(
                                        user_id,
                                        proto::ChannelRole::Talker,
                                        cx,
                                    )
                                })
                            })
                            .detach_and_prompt_err(
                                "Failed to grant mic access",
                                window,
                                cx,
                                |_, _, _| None,
                            )
                    }),
                );
            }
            if role == proto::ChannelRole::Guest || role == proto::ChannelRole::Talker {
                context_menu = context_menu.entry(
                    "Grant Write Access",
                    None,
                    window.handler_for(&this, move |_, window, cx| {
                        ActiveCall::global(cx)
                            .update(cx, |call, cx| {
                                let Some(room) = call.room() else {
                                    return Task::ready(Ok(()));
                                };
                                room.update(cx, |room, cx| {
                                    room.set_participant_role(
                                        user_id,
                                        proto::ChannelRole::Member,
                                        cx,
                                    )
                                })
                            })
                            .detach_and_prompt_err("Failed to grant write access", window, cx, |e, _, _| {
                                match e.error_code() {
                                    ErrorCode::NeedsCla => Some("This user has not yet signed the CLA at https://mav.dev/cla.".into()),
                                    _ => None,
                                }
                            })
                    }),
                );
            }
            if role == proto::ChannelRole::Member || role == proto::ChannelRole::Talker {
                let label = if role == proto::ChannelRole::Talker {
                    "Mute"
                } else {
                    "Revoke Access"
                };
                context_menu = context_menu.entry(
                    label,
                    None,
                    window.handler_for(&this, move |_, window, cx| {
                        ActiveCall::global(cx)
                            .update(cx, |call, cx| {
                                let Some(room) = call.room() else {
                                    return Task::ready(Ok(()));
                                };
                                room.update(cx, |room, cx| {
                                    room.set_participant_role(
                                        user_id,
                                        proto::ChannelRole::Guest,
                                        cx,
                                    )
                                })
                            })
                            .detach_and_prompt_err(
                                "Failed to revoke access",
                                window,
                                cx,
                                |_, _, _| None,
                            )
                    }),
                );
            }

            context_menu
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu.0.focus_handle(cx).contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some((context_menu, position, subscription));
    }

    fn deploy_channel_context_menu(
        &mut self,
        position: Point<Pixels>,
        channel_id: ChannelId,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clipboard_channel_name = self.channel_clipboard.as_ref().and_then(|clipboard| {
            self.channel_store
                .read(cx)
                .channel_for_id(clipboard.channel_id)
                .map(|channel| channel.name.clone())
        });
        let this = cx.entity();

        let context_menu = ContextMenu::build(window, cx, |mut context_menu, window, cx| {
            if self.has_subchannels(ix) {
                let expand_action_name = if self.is_channel_collapsed(channel_id) {
                    "Expand Subchannels"
                } else {
                    "Collapse Subchannels"
                };
                context_menu = context_menu.entry(
                    expand_action_name,
                    None,
                    window.handler_for(&this, move |this, window, cx| {
                        this.toggle_channel_collapsed(channel_id, window, cx)
                    }),
                );
            }

            context_menu = context_menu
                .entry(
                    "Open Notes",
                    None,
                    window.handler_for(&this, move |this, window, cx| {
                        this.open_channel_notes(channel_id, window, cx)
                    }),
                )
                .entry(
                    "Copy Channel Link",
                    None,
                    window.handler_for(&this, move |this, _, cx| {
                        this.copy_channel_link(channel_id, cx)
                    }),
                )
                .entry(
                    "Copy Channel Notes Link",
                    None,
                    window.handler_for(&this, move |this, _, cx| {
                        this.copy_channel_notes_link(channel_id, cx)
                    }),
                )
                .separator()
                .entry(
                    if self.is_channel_favorited(channel_id, cx) {
                        "Remove from Favorites"
                    } else {
                        "Add to Favorites"
                    },
                    None,
                    window.handler_for(&this, move |this, _window, cx| {
                        this.toggle_favorite_channel(channel_id, cx)
                    }),
                );

            let mut has_destructive_actions = false;
            if self.channel_store.read(cx).is_channel_admin(channel_id) {
                has_destructive_actions = true;
                context_menu = context_menu
                    .separator()
                    .entry(
                        "New Subchannel",
                        None,
                        window.handler_for(&this, move |this, window, cx| {
                            this.new_subchannel(channel_id, window, cx)
                        }),
                    )
                    .entry(
                        "Rename",
                        Some(Box::new(SecondaryConfirm)),
                        window.handler_for(&this, move |this, window, cx| {
                            this.rename_channel(channel_id, window, cx)
                        }),
                    );

                if let Some(channel_name) = clipboard_channel_name {
                    context_menu = context_menu.separator().entry(
                        format!("Move '#{}' here", channel_name),
                        None,
                        window.handler_for(&this, move |this, window, cx| {
                            this.move_channel_on_clipboard(channel_id, window, cx)
                        }),
                    );
                }

                if self.channel_store.read(cx).is_root_channel(channel_id) {
                    context_menu = context_menu.separator().entry(
                        "Manage Members",
                        None,
                        window.handler_for(&this, move |this, window, cx| {
                            this.manage_members(channel_id, window, cx)
                        }),
                    )
                } else {
                    context_menu = context_menu.entry(
                        "Move this channel",
                        None,
                        window.handler_for(&this, move |this, window, cx| {
                            this.start_move_channel(channel_id, window, cx)
                        }),
                    );
                    if self.channel_store.read(cx).is_public_channel(channel_id) {
                        context_menu = context_menu.separator().entry(
                            "Make Channel Private",
                            None,
                            window.handler_for(&this, move |this, window, cx| {
                                this.set_channel_visibility(
                                    channel_id,
                                    ChannelVisibility::Members,
                                    window,
                                    cx,
                                )
                            }),
                        )
                    } else {
                        context_menu = context_menu.separator().entry(
                            "Make Channel Public",
                            None,
                            window.handler_for(&this, move |this, window, cx| {
                                this.set_channel_visibility(
                                    channel_id,
                                    ChannelVisibility::Public,
                                    window,
                                    cx,
                                )
                            }),
                        )
                    }
                }

                context_menu = context_menu.entry(
                    "Delete",
                    None,
                    window.handler_for(&this, move |this, window, cx| {
                        this.remove_channel(channel_id, window, cx)
                    }),
                );
            }

            if self.channel_store.read(cx).is_root_channel(channel_id) {
                if !has_destructive_actions {
                    context_menu = context_menu.separator()
                }
                context_menu = context_menu.entry(
                    "Leave Channel",
                    None,
                    window.handler_for(&this, move |this, window, cx| {
                        this.leave_channel(channel_id, window, cx)
                    }),
                );
            }

            context_menu
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu.0.focus_handle(cx).contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some((context_menu, position, subscription));

        cx.notify();
    }
}
