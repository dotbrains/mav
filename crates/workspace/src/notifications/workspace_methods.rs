use std::time::Duration;

use gpui::{AnyEntity, AnyView, AppContext as _, Context, DismissEvent, Entity};
use project::project_settings::ProjectSettings;
use settings::Settings;
use ui::FluentBuilder;

use crate::{Toast, Workspace, workspace_error::WorkspaceError};

use super::{
    Notification, NotificationId, SuppressEvent, app_notifications::GLOBAL_APP_NOTIFICATIONS,
    language_server_prompt::LanguageServerPrompt, simple_message_notification,
};

impl Workspace {
    #[cfg(any(test, feature = "test-support"))]
    pub fn notification_ids(&self) -> Vec<NotificationId> {
        self.notifications
            .iter()
            .map(|(id, _)| id)
            .cloned()
            .collect()
    }

    pub fn show_notification<V: Notification>(
        &mut self,
        id: NotificationId,
        cx: &mut Context<Self>,
        build_notification: impl FnOnce(&mut Context<Self>) -> Entity<V>,
    ) {
        self.show_notification_without_handling_dismiss_events(&id, cx, |cx| {
            let notification = build_notification(cx);
            cx.subscribe(&notification, {
                let id = id.clone();
                move |this, _, _: &DismissEvent, cx| {
                    this.dismiss_notification(&id, cx);
                }
            })
            .detach();
            cx.subscribe(&notification, {
                let id = id.clone();
                move |workspace: &mut Workspace, _, _: &SuppressEvent, cx| {
                    workspace.suppress_notification(&id, cx);
                }
            })
            .detach();

            if let Ok(prompt) =
                AnyEntity::from(notification.clone()).downcast::<LanguageServerPrompt>()
            {
                let is_prompt_without_actions = prompt
                    .read(cx)
                    .request
                    .as_ref()
                    .is_some_and(|request| request.actions.is_empty());

                let dismiss_timeout_ms = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .notifications
                    .dismiss_timeout_ms;

                if is_prompt_without_actions {
                    if let Some(dismiss_duration_ms) = dismiss_timeout_ms.filter(|&ms| ms > 0) {
                        let task = cx.spawn({
                            let id = id.clone();
                            async move |this, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(dismiss_duration_ms))
                                    .await;
                                let _ = this.update(cx, |workspace, cx| {
                                    workspace.dismiss_notification(&id, cx);
                                });
                            }
                        });
                        prompt.update(cx, |prompt, _| {
                            prompt.dismiss_task = Some(task);
                        });
                    }
                }
            }
            notification.into()
        });
    }

    /// Shows a notification in this workspace's window. Caller must handle dismiss.
    ///
    /// This exists so that the `build_notification` closures stored for app notifications can
    /// return `AnyView`. Subscribing to events from an `AnyView` is not supported, so instead that
    /// responsibility is pushed to the caller where the `V` type is known.
    pub(crate) fn show_notification_without_handling_dismiss_events(
        &mut self,
        id: &NotificationId,
        cx: &mut Context<Self>,
        build_notification: impl FnOnce(&mut Context<Self>) -> AnyView,
    ) {
        if self.suppressed_notifications.contains(id) {
            return;
        }
        self.dismiss_notification(id, cx);
        self.notifications
            .push((id.clone(), build_notification(cx)));
        cx.notify();
    }

    pub fn show_error<E: WorkspaceError + 'static>(&mut self, err: E, cx: &mut Context<Self>) {
        self.show_notification(NotificationId::unique::<E>(), cx, |cx| {
            cx.new(|cx| {
                simple_message_notification::MessageNotification::from_workspace_error(err, cx)
            })
        });
    }

    pub fn dismiss_notification(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.notifications.retain(|(existing_id, _)| {
            if existing_id == id {
                cx.notify();
                false
            } else {
                true
            }
        });
    }

    pub fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.dismiss_notification(&toast.id, cx);
        self.show_notification(toast.id.clone(), cx, |cx| {
            cx.new(|cx| {
                simple_message_notification::MessageNotification::new(toast.message, cx).when_some(
                    toast.on_click,
                    |this, (click_msg, on_click)| {
                        this.primary_message(click_msg)
                            .primary_on_click(move |window, cx| on_click(window, cx))
                    },
                )
            })
        });

        if toast.autohide {
            cx.spawn(async move |workspace, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(5000))
                    .await;
                workspace
                    .update(cx, |workspace, cx| workspace.dismiss_toast(&toast.id, cx))
                    .ok();
            })
            .detach();
        }
    }

    pub fn dismiss_toast(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.dismiss_notification(id, cx);
    }

    pub fn clear_all_notifications(&mut self, cx: &mut Context<Self>) {
        self.notifications.clear();
        cx.notify();
    }

    /// Hide all notifications matching the given ID
    pub fn suppress_notification(&mut self, id: &NotificationId, cx: &mut Context<Self>) {
        self.dismiss_notification(id, cx);
        self.suppressed_notifications.insert(id.clone());
    }

    pub fn is_notification_suppressed(&self, notification_id: NotificationId) -> bool {
        self.suppressed_notifications.contains(&notification_id)
    }

    pub fn unsuppress(&mut self, notification_id: NotificationId) {
        self.suppressed_notifications.remove(&notification_id);
    }

    pub fn show_initial_notifications(&mut self, cx: &mut Context<Self>) {
        // Allow absence of the global so that tests don't need to initialize it.
        let app_notifications = GLOBAL_APP_NOTIFICATIONS
            .lock()
            .app_notifications
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for (id, build_notification) in app_notifications {
            self.show_notification_without_handling_dismiss_events(&id, cx, |cx| {
                build_notification(cx)
            });
        }
    }
}
