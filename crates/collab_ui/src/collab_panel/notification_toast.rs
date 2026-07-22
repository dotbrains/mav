use super::*;

impl CollabPanel {
    pub(super) fn on_notification_event(
        &mut self,
        _: &Entity<NotificationStore>,
        event: &NotificationEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            NotificationEvent::NewNotification { entry } => {
                self.add_toast(entry, cx);
                cx.notify();
            }
            NotificationEvent::NotificationRemoved { entry }
            | NotificationEvent::NotificationRead { entry } => {
                self.remove_toast(entry.id, cx);
                cx.notify();
            }
            NotificationEvent::NotificationsUpdated { .. } => {
                cx.notify();
            }
        }
    }

    pub(super) fn present_notification(
        &self,
        entry: &NotificationEntry,
        cx: &App,
    ) -> Option<(Option<Arc<User>>, String)> {
        let user_store = self.user_store.read(cx);
        match &entry.notification {
            Notification::ContactRequest { sender_id } => {
                let requester = user_store.get_cached_user(*sender_id)?;
                Some((
                    Some(requester.clone()),
                    format!("{} wants to add you as a contact", requester.username),
                ))
            }
            Notification::ContactRequestAccepted { responder_id } => {
                let responder = user_store.get_cached_user(*responder_id)?;
                Some((
                    Some(responder.clone()),
                    format!("{} accepted your contact request", responder.username),
                ))
            }
            Notification::ChannelInvitation {
                channel_name,
                inviter_id,
                ..
            } => {
                let inviter = user_store.get_cached_user(*inviter_id)?;
                Some((
                    Some(inviter.clone()),
                    format!(
                        "{} invited you to join the #{channel_name} channel",
                        inviter.username
                    ),
                ))
            }
        }
    }

    pub(super) fn add_toast(&mut self, entry: &NotificationEntry, cx: &mut Context<Self>) {
        let Some((actor, text)) = self.present_notification(entry, cx) else {
            return;
        };

        let notification = entry.notification.clone();
        let needs_response = matches!(
            notification,
            Notification::ContactRequest { .. } | Notification::ChannelInvitation { .. }
        );

        let notification_id = entry.id;

        self.current_notification_toast = Some((
            notification_id,
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TOAST_DURATION).await;
                this.update(cx, |this, cx| this.remove_toast(notification_id, cx))
                    .ok();
            }),
        ));

        let collab_panel = cx.entity().downgrade();
        self.workspace
            .update(cx, |workspace, cx| {
                let id = NotificationId::unique::<CollabNotificationToast>();

                workspace.dismiss_notification(&id, cx);
                workspace.show_notification(id, cx, |cx| {
                    let workspace = cx.entity().downgrade();
                    cx.new(|cx| CollabNotificationToast {
                        actor,
                        text,
                        notification: needs_response.then(|| notification),
                        workspace,
                        collab_panel: collab_panel.clone(),
                        focus_handle: cx.focus_handle(),
                    })
                })
            })
            .ok();
    }

    pub(super) fn mark_notification_read(&mut self, notification_id: u64, cx: &mut Context<Self>) {
        let client = self.client.clone();
        self.mark_as_read_tasks
            .entry(notification_id)
            .or_insert_with(|| {
                cx.spawn(async move |this, cx| {
                    let request_result = client
                        .request(proto::MarkNotificationRead { notification_id })
                        .await;

                    this.update(cx, |this, _| {
                        this.mark_as_read_tasks.remove(&notification_id);
                    })?;

                    request_result?;
                    Ok(())
                })
            });
    }

    pub(super) fn mark_contact_request_accepted_notifications_read(
        &mut self,
        contact_user_id: u64,
        cx: &mut Context<Self>,
    ) {
        let notification_ids = self.notification_store.read_with(cx, |store, _| {
            (0..store.notification_count())
                .filter_map(|index| {
                    let entry = store.notification_at(index)?;
                    if entry.is_read {
                        return None;
                    }

                    match &entry.notification {
                        Notification::ContactRequestAccepted { responder_id }
                            if *responder_id == contact_user_id =>
                        {
                            Some(entry.id)
                        }
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        });

        for notification_id in notification_ids {
            self.mark_notification_read(notification_id, cx);
        }
    }

    pub(super) fn remove_toast(&mut self, notification_id: u64, cx: &mut Context<Self>) {
        if let Some((current_id, _)) = &self.current_notification_toast {
            if *current_id == notification_id {
                self.dismiss_toast(cx);
            }
        }
    }

    pub(super) fn dismiss_toast(&mut self, cx: &mut Context<Self>) {
        self.current_notification_toast.take();
        self.workspace
            .update(cx, |workspace, cx| {
                let id = NotificationId::unique::<CollabNotificationToast>();
                workspace.dismiss_notification(&id, cx)
            })
            .ok();
    }
}

pub struct CollabNotificationToast {
    actor: Option<Arc<User>>,
    text: String,
    notification: Option<Notification>,
    workspace: WeakEntity<Workspace>,
    collab_panel: WeakEntity<CollabPanel>,
    focus_handle: FocusHandle,
}

impl Focusable for CollabNotificationToast {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl WorkspaceNotification for CollabNotificationToast {}

impl CollabNotificationToast {
    fn focus_collab_panel(&self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        window.defer(cx, move |window, cx| {
            workspace
                .update(cx, |workspace, cx| {
                    workspace.focus_panel::<CollabPanel>(window, cx)
                })
                .ok();
        })
    }

    fn respond(&mut self, accept: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(notification) = self.notification.take() {
            self.collab_panel
                .update(cx, |collab_panel, cx| match notification {
                    Notification::ContactRequest { sender_id } => {
                        collab_panel.respond_to_contact_request(sender_id, accept, window, cx);
                    }
                    Notification::ChannelInvitation { channel_id, .. } => {
                        collab_panel.respond_to_channel_invite(ChannelId(channel_id), accept, cx);
                    }
                    Notification::ContactRequestAccepted { .. } => {}
                })
                .ok();
        }
        cx.emit(DismissEvent);
    }
}

impl Render for CollabNotificationToast {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let needs_response = self.notification.is_some();

        let accept_button = if needs_response {
            Button::new("accept", "Accept").on_click(cx.listener(|this, _, window, cx| {
                this.respond(true, window, cx);
                cx.stop_propagation();
            }))
        } else {
            Button::new("dismiss", "Dismiss").on_click(cx.listener(|_, _, _, cx| {
                cx.emit(DismissEvent);
            }))
        };

        let decline_button = if needs_response {
            Button::new("decline", "Decline").on_click(cx.listener(|this, _, window, cx| {
                this.respond(false, window, cx);
                cx.stop_propagation();
            }))
        } else {
            Button::new("close", "Close").on_click(cx.listener(|_, _, _, cx| {
                cx.emit(DismissEvent);
            }))
        };

        let avatar_uri = self
            .actor
            .as_ref()
            .map(|user| user.avatar_uri.clone())
            .unwrap_or_default();

        div()
            .id("collab_notification_toast")
            .on_click(cx.listener(|this, _, window, cx| {
                this.focus_collab_panel(window, cx);
                cx.emit(DismissEvent);
            }))
            .child(
                CollabNotification::new(avatar_uri, accept_button, decline_button)
                    .child(Label::new(self.text.clone())),
            )
    }
}

impl EventEmitter<DismissEvent> for CollabNotificationToast {}
impl EventEmitter<SuppressEvent> for CollabNotificationToast {}
