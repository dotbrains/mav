mod channel_modal;
#[path = "collab_panel/contact_actions.rs"]
mod contact_actions;
mod contact_finder;
#[path = "collab_panel/init.rs"]
mod init;
mod list_entry;
mod notification_toast;
#[path = "collab_panel/panel_state.rs"]
mod panel_state;
#[path = "collab_panel/panel_traits.rs"]
mod panel_traits;
#[path = "collab_panel/render_helpers.rs"]
mod render_helpers;
#[cfg(any(test, feature = "test-support"))]
#[path = "collab_panel/test_support.rs"]
mod test_support;

use self::channel_modal::ChannelModal;
use self::list_entry::{ListEntry, Section};
use self::notification_toast::CollabNotificationToast;
use self::panel_state::*;
use self::render_helpers::{
    DraggedChannelView, JoinChannelTooltip, render_participant_name_and_handle, render_tree_branch,
};
use crate::{CollaborationPanelSettings, channel_view::ChannelView};
use anyhow::Context as _;
use call::ActiveCall;
use channel::{Channel, ChannelEvent, ChannelStore};
use client::{ChannelId, Client, Contact, Notification, User, UserStore};
use collections::{HashMap, HashSet};
use contact_finder::ContactFinder;
use db::kvp::KeyValueStore;
use editor::{Editor, EditorElement, EditorStyle};
use fuzzy::{StringMatch, StringMatchCandidate, match_strings};
use gpui::{
    AnyElement, App, AsyncWindowContext, Bounds, ClickEvent, ClipboardItem, DismissEvent, Div,
    Empty, Entity, EventEmitter, FocusHandle, Focusable, FontStyle, KeyContext, ListOffset,
    ListState, MouseDownEvent, Pixels, Point, PromptLevel, SharedString, Subscription, Task,
    TextStyle, WeakEntity, Window, actions, anchored, canvas, deferred, div, fill, list, point,
    prelude::*, px,
};

use menu::{Cancel, Confirm, SecondaryConfirm, SelectNext, SelectPrevious};
use notifications::{NotificationEntry, NotificationEvent, NotificationStore};
use project::{Fs, Project};
use rpc::{
    ErrorCode, ErrorExt,
    proto::{self, ChannelVisibility, PeerId, reorder_channel::Direction},
};
use serde::{Deserialize, Serialize};
use settings::Settings;
use smallvec::SmallVec;
use std::{mem, sync::Arc, time::Duration};
use theme::ActiveTheme;
use theme_settings::ThemeSettings;
use ui::{
    Avatar, AvatarAvailabilityIndicator, CollabNotification, ContextMenu, CopyButton, Facepile,
    HighlightedLabel, IconButtonShape, Indicator, ListHeader, ListItem, Tab, TintColor, Tooltip,
    prelude::*, tooltip_container,
};
use util::{ResultExt, TryFutureExt, maybe};
use workspace::{
    AutoWatch, CopyRoomId, Deafen, LeaveCall, MultiWorkspace, Mute, OpenChannelNotes,
    OpenChannelNotesById, ScreenShare, ShareProject, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::{
        DetachAndPromptErr, Notification as WorkspaceNotification, NotificationId, NotifyResultExt,
        SuppressEvent,
    },
};

actions!(
    collab_panel,
    [
        /// Toggles the collab panel.
        Toggle,
        /// Toggles focus on the collaboration panel.
        ToggleFocus,
        /// Removes the selected channel or contact.
        Remove,
        /// Opens the context menu for the selected item.
        Secondary,
        /// Collapses the selected channel in the tree view.
        CollapseSelectedChannel,
        /// Expands the selected channel in the tree view.
        ExpandSelectedChannel,
        /// Opens the meeting notes for the selected channel in the panel.
        ///
        /// Use `collab::OpenChannelNotes` to open the channel notes for the current call.
        OpenSelectedChannelNotes,
        /// Toggles whether the selected channel is in the Favorites section.
        ToggleSelectedChannelFavorite,
        /// Starts moving a channel to a new location.
        StartMoveChannel,
        /// Moves the selected item to the current location.
        MoveSelected,
        /// Inserts a space character in the filter input.
        InsertSpace,
        /// Moves the selected channel up in the list.
        MoveChannelUp,
        /// Moves the selected channel down in the list.
        MoveChannelDown,
    ]
);

pub fn init(cx: &mut App) {
    init::init(cx);
}

pub struct CollabPanel {
    fs: Arc<dyn Fs>,
    focus_handle: FocusHandle,
    channel_clipboard: Option<ChannelMoveClipboard>,
    pending_panel_serialization: Task<Option<()>>,
    pending_favorites_serialization: Task<Option<()>>,
    pending_filter_serialization: Task<Option<()>>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    list_state: ListState,
    filter_editor: Entity<Editor>,
    channel_name_editor: Entity<Editor>,
    channel_editing_state: Option<ChannelEditingState>,
    entries: Vec<ListEntry>,
    selection: Option<usize>,
    channel_store: Entity<ChannelStore>,
    user_store: Entity<UserStore>,
    client: Arc<Client>,
    project: Entity<Project>,
    match_candidates: Vec<StringMatchCandidate>,
    subscriptions: Vec<Subscription>,
    collapsed_sections: Vec<Section>,
    collapsed_channels: Vec<ChannelId>,
    filter_occupied_channels: bool,
    workspace: WeakEntity<Workspace>,
    notification_store: Entity<NotificationStore>,
    current_notification_toast: Option<(u64, Task<()>)>,
    mark_as_read_tasks: HashMap<u64, Task<anyhow::Result<()>>>,
}

#[path = "collab_panel/channel_actions.rs"]
mod channel_actions;
#[path = "collab_panel/channel_edit.rs"]
mod channel_edit;
#[path = "collab_panel/collapse_favorites.rs"]
mod collapse_favorites;
#[path = "collab_panel/context_menus.rs"]
mod context_menus;
#[path = "collab_panel/entries.rs"]
mod entries;
#[path = "collab_panel/entries_selection_update.rs"]
mod entries_selection_update;
#[path = "collab_panel/lifecycle.rs"]
mod lifecycle;
#[path = "collab_panel/navigation.rs"]
mod navigation;
#[path = "collab_panel/participant_render.rs"]
mod participant_render;
#[path = "collab_panel/render_entries.rs"]
mod render_entries;
#[path = "collab_panel/render_header.rs"]
mod render_header;
#[path = "collab_panel/render_states.rs"]
mod render_states;
#[path = "collab_panel/selection.rs"]
mod selection;
