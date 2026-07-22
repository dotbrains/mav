mod app_notifications;
mod language_server_prompt;
mod markdown_style;
mod notify_ext;
mod workspace_methods;

pub mod simple_message_notification;

#[cfg(test)]
mod tests;

use std::{any::TypeId, ops::Deref};

use gpui::{AnyView, DismissEvent, EventEmitter, Focusable, Render};
use ui::prelude::*;

pub use app_notifications::{dismiss_app_notification, show_app_notification};
pub use language_server_prompt::LanguageServerPrompt;
pub use notify_ext::{DetachAndPromptErr, NotifyResultExt, NotifyTaskExt};

#[derive(Default)]
pub struct Notifications {
    notifications: Vec<(NotificationId, AnyView)>,
}

impl Deref for Notifications {
    type Target = Vec<(NotificationId, AnyView)>;

    fn deref(&self) -> &Self::Target {
        &self.notifications
    }
}

impl std::ops::DerefMut for Notifications {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.notifications
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum NotificationId {
    Unique(TypeId),
    Composite(TypeId, ElementId),
    Named(SharedString),
}

impl NotificationId {
    /// Returns a unique [`NotificationId`] for the given type.
    pub const fn unique<T: 'static>() -> Self {
        Self::Unique(TypeId::of::<T>())
    }

    /// Returns a [`NotificationId`] for the given type that is also identified
    /// by the provided ID.
    pub fn composite<T: 'static>(id: impl Into<ElementId>) -> Self {
        Self::Composite(TypeId::of::<T>(), id.into())
    }

    /// Builds a `NotificationId` out of the given string.
    pub fn named(id: SharedString) -> Self {
        Self::Named(id)
    }
}

pub trait Notification:
    EventEmitter<DismissEvent> + EventEmitter<SuppressEvent> + Focusable + Render
{
}

pub struct SuppressEvent;

fn workspace_error_notification_id() -> NotificationId {
    struct WorkspaceErrorNotification;
    NotificationId::unique::<WorkspaceErrorNotification>()
}
