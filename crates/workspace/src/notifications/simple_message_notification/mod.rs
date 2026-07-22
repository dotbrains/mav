mod auto_hide;
mod message;
mod preview;
mod render;

use std::sync::Arc;

use gpui::{
    AnyElement, DismissEvent, EventEmitter, FocusHandle, Focusable, ParentElement, Render,
    ScrollHandle, SharedString, Styled,
};
use ui::{CopyButton, Tooltip, WithScrollbar, prelude::*};

use crate::{
    SuppressNotification,
    workspace_error::{ActionIcon, ErrorAction, ErrorActionHandler, ErrorSeverity, WorkspaceError},
};

use super::{Notification, SuppressEvent};

use auto_hide::AutoHideState;
pub use message::MessageNotification;
