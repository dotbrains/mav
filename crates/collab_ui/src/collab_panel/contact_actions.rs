use super::*;

impl CollabPanel {
    pub(super) fn deploy_contact_context_menu(
        &mut self,
        position: Point<Pixels>,
        contact: Arc<Contact>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity();
        let in_room = ActiveCall::global(cx).read(cx).room().is_some();

        let context_menu = ContextMenu::build(window, cx, |mut context_menu, _, _| {
            let user_id = contact.user.legacy_id;

            if contact.online && !contact.busy {
                let label = if in_room {
                    format!("Invite {} to join", contact.user.username)
                } else {
                    format!("Call {}", contact.user.username)
                };
                context_menu = context_menu.entry(label, None, {
                    let this = this.clone();
                    move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.call(user_id, window, cx);
                        });
                    }
                });
            }

            context_menu.entry("Remove Contact", None, {
                let this = this.clone();
                move |window, cx| {
                    this.update(cx, |this, cx| {
                        this.remove_contact(
                            contact.user.legacy_id,
                            &contact.user.username,
                            window,
                            cx,
                        );
                    });
                }
            })
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

    pub(super) fn remove_contact(
        &mut self,
        user_id: u64,
        github_login: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let user_store = self.user_store.clone();
        let prompt_message = format!(
            "Are you sure you want to remove \"{}\" from your contacts?",
            github_login
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            &prompt_message,
            None,
            &["Remove", "Cancel"],
            cx,
        );
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |_, mut cx| {
            if answer.await? == 0 {
                user_store
                    .update(cx, |store, cx| store.remove_contact(user_id, cx))
                    .await
                    .notify_workspace_async_err(workspace, &mut cx);
            }
            anyhow::Ok(())
        })
        .detach_and_prompt_err("Failed to remove contact", window, cx, |_, _, _| None);
    }

    pub(super) fn respond_to_contact_request(
        &mut self,
        user_id: u64,
        accept: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.user_store
            .update(cx, |store, cx| {
                store.respond_to_contact_request(user_id, accept, cx)
            })
            .detach_and_prompt_err(
                "Failed to respond to contact request",
                window,
                cx,
                |_, _, _| None,
            );
    }

    pub(super) fn render_contact(
        &self,
        contact: &Arc<Contact>,
        calling: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let online = contact.online;
        let busy = contact.busy || calling;
        let username = contact.user.username.clone();
        let item = ListItem::new(username.clone())
            .indent_level(1)
            .indent_step_size(px(20.))
            .toggle_state(is_selected)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(render_participant_name_and_handle(&contact.user))
                    .when(calling, |el| {
                        el.child(Label::new("Calling").color(Color::Muted))
                    })
                    .when(!calling, |el| {
                        el.child(
                            IconButton::new("contact context menu", IconName::Ellipsis)
                                .icon_color(Color::Muted)
                                .visible_on_hover("")
                                .on_click(cx.listener({
                                    let contact = contact.clone();
                                    move |this, event: &ClickEvent, window, cx| {
                                        this.deploy_contact_context_menu(
                                            event.position(),
                                            contact.clone(),
                                            window,
                                            cx,
                                        );
                                    }
                                })),
                        )
                    }),
            )
            .on_secondary_mouse_down(cx.listener({
                let contact = contact.clone();
                move |this, event: &MouseDownEvent, window, cx| {
                    this.deploy_contact_context_menu(event.position, contact.clone(), window, cx);
                }
            }))
            .start_slot(
                Avatar::new(contact.user.avatar_uri.clone())
                    .indicator::<AvatarAvailabilityIndicator>(if online {
                        Some(AvatarAvailabilityIndicator::new(match busy {
                            true => ui::CollaboratorAvailability::Busy,
                            false => ui::CollaboratorAvailability::Free,
                        }))
                    } else {
                        None
                    }),
            );

        div()
            .id(username.clone())
            .group("")
            .child(item)
            .tooltip(move |_, cx| {
                let text = if !online {
                    format!(" {} is offline", &username)
                } else if busy {
                    format!(" {} is on a call", &username)
                } else {
                    let room = ActiveCall::global(cx).read(cx).room();
                    if room.is_some() {
                        format!("Invite {} to join call", &username)
                    } else {
                        format!("Call {}", &username)
                    }
                };
                Tooltip::simple(text, cx)
            })
    }

    pub(super) fn render_contact_request(
        &self,
        user: &Arc<User>,
        is_incoming: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let username = user.username.clone();
        let user_id = user.legacy_id;
        let is_response_pending = self.user_store.read(cx).is_contact_request_pending(user);
        let color = if is_response_pending {
            Color::Muted
        } else {
            Color::Default
        };

        let controls = if is_incoming {
            vec![
                IconButton::new("decline-contact", IconName::Close)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.respond_to_contact_request(user_id, false, window, cx);
                    }))
                    .icon_color(color)
                    .tooltip(Tooltip::text("Decline invite")),
                IconButton::new("accept-contact", IconName::Check)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.respond_to_contact_request(user_id, true, window, cx);
                    }))
                    .icon_color(color)
                    .tooltip(Tooltip::text("Accept invite")),
            ]
        } else {
            let github_login = username.clone();
            vec![
                IconButton::new("remove_contact", IconName::Close)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.remove_contact(user_id, &github_login, window, cx);
                    }))
                    .icon_color(color)
                    .tooltip(Tooltip::text("Cancel invite")),
            ]
        };

        ListItem::new(username.clone())
            .indent_level(1)
            .indent_step_size(px(20.))
            .toggle_state(is_selected)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(Label::new(username))
                    .child(h_flex().children(controls)),
            )
            .start_slot(Avatar::new(user.avatar_uri.clone()))
    }

    pub(super) fn render_contact_placeholder(
        &self,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> ListItem {
        ListItem::new("contact-placeholder")
            .child(Icon::new(IconName::Plus))
            .child(Label::new("Add a Contact"))
            .toggle_state(is_selected)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_contact_finder(window, cx)))
    }
}
