use std::sync::{Arc, LazyLock};

use gpui::{AnyView, App, Context, DismissEvent, Entity};
use parking_lot::Mutex;

use crate::{MultiWorkspace, Workspace};

use super::{Notification, NotificationId, SuppressEvent};

pub(super) static GLOBAL_APP_NOTIFICATIONS: LazyLock<Mutex<AppNotifications>> =
    LazyLock::new(|| {
        Mutex::new(AppNotifications {
            app_notifications: Vec::new(),
        })
    });

/// Stores app notifications so that they can be shown in new workspaces.
pub(super) struct AppNotifications {
    pub(super) app_notifications: Vec<(
        NotificationId,
        Arc<dyn Fn(&mut Context<Workspace>) -> AnyView + Send + Sync>,
    )>,
}

impl AppNotifications {
    pub fn insert(
        &mut self,
        id: NotificationId,
        build_notification: Arc<dyn Fn(&mut Context<Workspace>) -> AnyView + Send + Sync>,
    ) {
        self.remove(&id);
        self.app_notifications.push((id, build_notification))
    }

    pub fn remove(&mut self, id: &NotificationId) {
        self.app_notifications
            .retain(|(existing_id, _)| existing_id != id);
    }
}

/// Shows a notification in all workspaces. New workspaces will also receive the notification - this
/// is particularly to handle notifications that occur on initialization before any workspaces
/// exist. If the notification is dismissed within any workspace, it will be removed from all.
pub fn show_app_notification<V: Notification + 'static>(
    id: NotificationId,
    cx: &mut App,
    build_notification: impl Fn(&mut Context<Workspace>) -> Entity<V> + 'static + Send + Sync,
) {
    // Defer notification creation so that windows on the stack can be returned to GPUI
    cx.defer(move |cx| {
        // Handle dismiss events by removing the notification from all workspaces.
        let build_notification: Arc<dyn Fn(&mut Context<Workspace>) -> AnyView + Send + Sync> =
            Arc::new({
                let id = id.clone();
                move |cx| {
                    let notification = build_notification(cx);
                    cx.subscribe(&notification, {
                        let id = id.clone();
                        move |_, _, _: &DismissEvent, cx| {
                            dismiss_app_notification(&id, cx);
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
                    notification.into()
                }
            });

        // Store the notification so that new workspaces also receive it.
        GLOBAL_APP_NOTIFICATIONS
            .lock()
            .insert(id.clone(), build_notification.clone());

        for window in cx.windows() {
            if let Some(multi_workspace) = window.downcast::<MultiWorkspace>() {
                multi_workspace
                    .update(cx, |multi_workspace, _window, cx| {
                        for workspace in multi_workspace.workspaces() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.show_notification_without_handling_dismiss_events(
                                    &id,
                                    cx,
                                    |cx| build_notification(cx),
                                );
                            });
                        }
                    })
                    .ok(); // Doesn't matter if the windows are dropped
            }
        }
    });
}

pub fn dismiss_app_notification(id: &NotificationId, cx: &mut App) {
    let id = id.clone();
    // Defer notification dismissal so that windows on the stack can be returned to GPUI
    cx.defer(move |cx| {
        GLOBAL_APP_NOTIFICATIONS.lock().remove(&id);
        for window in cx.windows() {
            if let Some(multi_workspace) = window.downcast::<MultiWorkspace>() {
                let id = id.clone();
                multi_workspace
                    .update(cx, |multi_workspace, _window, cx| {
                        for workspace in multi_workspace.workspaces() {
                            workspace.update(cx, |workspace, cx| {
                                workspace.dismiss_notification(&id, cx)
                            });
                        }
                    })
                    .ok();
            }
        }
    });
}
