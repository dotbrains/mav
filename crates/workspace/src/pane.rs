use crate::{
    CloseWindow, NewCenterTerminal, NewFile, NewTerminal, OpenInTerminal, OpenOptions,
    OpenTerminal, OpenVisible, SidebarSide, SplitDirection, ToggleFileFinder, ToggleProjectSymbols,
    ToggleZoom, Workspace, WorkspaceItemBuilder, ZoomIn, ZoomOut,
    focus_follows_mouse::FocusFollowsMouse as _,
    invalid_item_view::InvalidItemView,
    item::{
        ActivateOnClose, ClosePosition, Item, ItemBufferKind, ItemHandle, ItemSettings,
        PreviewTabsSettings, ProjectItemKind, SaveOptions, ShowCloseButton, ShowDiagnostics,
        TabContentParams, TabTooltipContent, WeakItemHandle,
    },
    move_item,
    notifications::NotifyResultExt,
    render_sidebar_header_controls_with_project_pane_visibility,
    toolbar::Toolbar,
    workspace_settings::{AutosaveSetting, FocusFollowsMouse, TabBarSettings, WorkspaceSettings},
};
use anyhow::Result;
use collections::{BTreeSet, HashMap, HashSet};
use futures::{StreamExt, stream::FuturesUnordered};
use gpui::{
    Action, Anchor, AnyElement, App, AsyncWindowContext, ClickEvent, ClipboardItem, Context,
    DragMoveEvent, Entity, EntityId, EventEmitter, ExternalPaths, FocusHandle, FocusOutEvent,
    Focusable, KeyContext, MouseButton, NavigationDirection, Pixels, Point, PromptLevel, Render,
    ScrollHandle, Subscription, Task, TaskExt, WeakEntity, WeakFocusHandle, Window, actions,
    prelude::*,
};
use itertools::Itertools;
use language::{Capability, DiagnosticSeverity};
use project::{DirectoryLister, Project, ProjectEntryId, ProjectPath, WorktreeId};
use schemars::JsonSchema;
use serde::Deserialize;
use settings::{Settings, SettingsStore};
use std::{
    any::Any,
    cmp, fmt, mem,
    num::NonZeroUsize,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

mod close_commands;
mod close_flow;
mod drag_state;
mod dragged_render;
mod drop_handlers;
mod focus_events;
mod item_accessors;
mod item_activation;
mod navigation_history;
mod open_items;
mod pane_actions;
mod pane_construction;
mod pane_header;
mod pane_helpers;
mod pane_layout;
mod pane_navigation;
mod pane_render;
mod pane_settings;
mod preview_items;
mod save_items;
mod tab_bar_render;
mod tab_context_menu;
mod tab_item_render;
mod tab_operations;
pub use navigation_history::{
    ItemNavHistory, NavHistory, NavigationEntry, NavigationMode, TagNavigationMode,
};
pub use pane_actions::*;
pub(super) use pane_helpers::dirty_message_for;
pub(crate) use pane_helpers::render_toggle_zoom_button;
pub use pane_helpers::{render_item_indicator, tab_details};
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, ContextMenuEntry, ContextMenuItem, DecoratedIcon, IconButtonShape, IconDecoration,
    IconDecorationKind, Indicator, PopoverMenu, PopoverMenuHandle, Tab, TabBar, TabPosition,
    Tooltip, prelude::*, right_click_menu,
};
use util::{
    ResultExt, debug_panic, maybe, paths::PathStyle, serde::default_true, truncate_and_remove_front,
};

/// A selected entry in e.g. project panel.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SelectedEntry {
    pub worktree_id: WorktreeId,
    pub entry_id: ProjectEntryId,
}

/// A group of selected entries from project panel.
#[derive(Debug)]
pub struct DraggedSelection {
    pub active_selection: SelectedEntry,
    pub marked_selections: Arc<[SelectedEntry]>,
    pub source_pane: Option<WeakEntity<Pane>>,
    pub active_selection_is_file: bool,
}

impl DraggedSelection {
    pub fn items<'a>(&'a self) -> Box<dyn Iterator<Item = &'a SelectedEntry> + 'a> {
        if self.marked_selections.contains(&self.active_selection) {
            Box::new(self.marked_selections.iter())
        } else {
            Box::new(std::iter::once(&self.active_selection))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PaneKind {
    #[default]
    Tabs,
    Project,
    Agent,
}

impl PaneKind {
    pub fn is_tabbed(self) -> bool {
        self == Self::Tabs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabInsertionSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabInsertionTarget {
    Tab { ix: usize, side: TabInsertionSide },
    UnpinnedEnd,
    PinnedEnd,
}

const MAX_NAVIGATION_HISTORY_LEN: usize = 1024;

pub enum Event {
    AddItem {
        item: Box<dyn ItemHandle>,
    },
    ActivateItem {
        local: bool,
        focus_changed: bool,
    },
    Remove {
        focus_on_pane: Option<Entity<Pane>>,
    },
    RemovedItem {
        item: Box<dyn ItemHandle>,
    },
    Split {
        direction: SplitDirection,
        mode: SplitMode,
    },
    ItemPinned,
    ItemUnpinned,
    JoinAll,
    JoinIntoNext,
    ChangeItemTitle,
    Focus,
    ZoomIn,
    ZoomOut,
    UserSavedItem {
        item: Box<dyn WeakItemHandle>,
        save_intent: SaveIntent,
    },
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::AddItem { item } => f
                .debug_struct("AddItem")
                .field("item", &item.item_id())
                .finish(),
            Event::ActivateItem { local, .. } => f
                .debug_struct("ActivateItem")
                .field("local", local)
                .finish(),
            Event::Remove { .. } => f.write_str("Remove"),
            Event::RemovedItem { item } => f
                .debug_struct("RemovedItem")
                .field("item", &item.item_id())
                .finish(),
            Event::Split { direction, mode } => f
                .debug_struct("Split")
                .field("direction", direction)
                .field("mode", mode)
                .finish(),
            Event::JoinAll => f.write_str("JoinAll"),
            Event::JoinIntoNext => f.write_str("JoinIntoNext"),
            Event::ChangeItemTitle => f.write_str("ChangeItemTitle"),
            Event::Focus => f.write_str("Focus"),
            Event::ZoomIn => f.write_str("ZoomIn"),
            Event::ZoomOut => f.write_str("ZoomOut"),
            Event::UserSavedItem { item, save_intent } => f
                .debug_struct("UserSavedItem")
                .field("item", &item.id())
                .field("save_intent", save_intent)
                .finish(),
            Event::ItemPinned => f.write_str("ItemPinned"),
            Event::ItemUnpinned => f.write_str("ItemUnpinned"),
        }
    }
}

/// A container for 0 to many items that are open in the workspace.
/// Treats all items uniformly via the [`ItemHandle`] trait, whether it's an editor, search results multibuffer, terminal or something else,
/// responsible for managing item tabs, focus and zoom states and drag and drop features.
/// Can be split, see `PaneGroup` for more details.
pub struct Pane {
    alternate_file_items: (
        Option<Box<dyn WeakItemHandle>>,
        Option<Box<dyn WeakItemHandle>>,
    ),
    focus_handle: FocusHandle,
    items: Vec<Box<dyn ItemHandle>>,
    activation_history: Vec<ActivationHistoryEntry>,
    next_activation_timestamp: Arc<AtomicUsize>,
    zoomed: bool,
    was_focused: bool,
    active_item_index: usize,
    preview_item_id: Option<EntityId>,
    last_focus_handle_by_item: HashMap<EntityId, WeakFocusHandle>,
    nav_history: NavHistory,
    toolbar: Entity<Toolbar>,
    pub(crate) workspace: WeakEntity<Workspace>,
    project: WeakEntity<Project>,
    pub drag_split_direction: Option<SplitDirection>,
    drag_swap_target: bool,
    drag_tab_target: bool,
    drag_tab_insertion_target: Option<TabInsertionTarget>,
    can_drop_predicate: Option<Arc<dyn Fn(&dyn Any, &mut Window, &mut App) -> bool>>,
    can_split_predicate:
        Option<Arc<dyn Fn(&mut Self, &dyn Any, &mut Window, &mut Context<Self>) -> bool>>,
    can_toggle_zoom: bool,
    should_display_tab_bar: Rc<dyn Fn(&Window, &mut Context<Pane>) -> bool>,
    should_display_welcome_page: bool,
    render_tab_bar_buttons: Rc<
        dyn Fn(
            &mut Pane,
            &mut Window,
            &mut Context<Pane>,
        ) -> (Option<AnyElement>, Option<AnyElement>),
    >,
    render_tab_bar: Rc<dyn Fn(&mut Pane, &mut Window, &mut Context<Pane>) -> AnyElement>,
    show_tab_bar_buttons: bool,
    max_tabs: Option<NonZeroUsize>,
    use_max_tabs: bool,
    _subscriptions: Vec<Subscription>,
    tab_bar_scroll_handle: ScrollHandle,
    /// This is set to true if a user scroll has occurred more recently than a system scroll
    /// We want to suppress certain system scrolls when the user has intentionally scrolled
    suppress_scroll: bool,
    /// Is None if navigation buttons are permanently turned off (and should not react to setting changes).
    /// Otherwise, when `display_nav_history_buttons` is Some, it determines whether nav buttons should be displayed.
    display_nav_history_buttons: Option<bool>,
    double_click_dispatch_action: Box<dyn Action>,
    save_modals_spawned: HashSet<EntityId>,
    close_pane_if_empty: bool,
    pub new_item_context_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub split_item_context_menu_handle: PopoverMenuHandle<ContextMenu>,
    pinned_tab_count: usize,
    diagnostics: HashMap<ProjectPath, DiagnosticSeverity>,
    zoom_out_on_close: bool,
    focus_follows_mouse: FocusFollowsMouse,
    diagnostic_summary_update: Task<()>,
    /// If a certain project item wants to get recreated with specific data, it can persist its data before the recreation here.
    pub project_item_restoration_data: HashMap<ProjectItemKind, Box<dyn Any + Send>>,
    welcome_page: Option<Entity<crate::welcome::WelcomePage>>,

    pub in_center_group: bool,
    reserve_traffic_light_space: bool,
    pane_kind: PaneKind,
    visible: bool,
    preferred_horizontal_split_size: Option<Pixels>,
}

pub struct ActivationHistoryEntry {
    pub entity_id: EntityId,
    pub timestamp: usize,
}

#[derive(Clone)]
pub struct DraggedTab {
    pub pane: Entity<Pane>,
    pub item: Box<dyn ItemHandle>,
    pub ix: usize,
    pub detail: usize,
    pub is_active: bool,
}

#[derive(Clone)]
pub struct DraggedPane {
    pub pane: Entity<Pane>,
}

impl EventEmitter<Event> for Pane {}

pub enum Side {
    Left,
    Right,
}

#[derive(Copy, Clone)]
enum PinOperation {
    Pin,
    Unpin,
}

impl Pane {}

#[cfg(test)]
mod tests;
