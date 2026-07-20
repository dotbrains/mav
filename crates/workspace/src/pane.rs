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
mod drop_handlers;
mod item_accessors;
mod item_activation;
mod navigation_history;
mod open_items;
mod pane_header;
mod pane_render;
mod preview_items;
mod tab_bar_render;
mod tab_context_menu;
mod tab_item_render;
mod tab_operations;
pub use navigation_history::{
    ItemNavHistory, NavHistory, NavigationEntry, NavigationMode, TagNavigationMode,
};
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

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveIntent {
    /// write all files (even if unchanged)
    /// prompt before overwriting on-disk changes
    Save,
    /// same as Save, but always formats regardless of the format_on_save setting
    FormatAndSave,
    /// same as Save, but without auto formatting
    SaveWithoutFormat,
    /// write any files that have local changes
    /// prompt before overwriting on-disk changes
    SaveAll,
    /// always prompt for a new path
    SaveAs,
    /// prompt "you have unsaved changes" before writing
    Close,
    /// write all dirty files, don't prompt on conflict
    Overwrite,
    /// skip all save-related behavior
    Skip,
}

/// Activates a specific item in the pane by its index.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
pub struct ActivateItem(pub usize);

/// Closes the currently active item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseActiveItem {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all inactive items in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
#[action(deprecated_aliases = ["pane::CloseInactiveItems"])]
pub struct CloseOtherItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all multibuffers in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseMultibufferItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseAllItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items that have no unsaved changes.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseCleanItems {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items to the right of the current item.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseItemsToTheRight {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Closes all items to the left of the current item.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseItemsToTheLeft {
    #[serde(default)]
    pub close_pinned: bool,
}

/// Reveals the current item in the project panel.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct RevealInProjectPanel {
    #[serde(skip)]
    pub entry_id: Option<u64>,
}

/// Opens the search interface with the specified configuration.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct DeploySearch {
    #[serde(default)]
    pub replace_enabled: bool,
    #[serde(default)]
    pub included_files: Option<String>,
    #[serde(default)]
    pub excluded_files: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub regex: Option<bool>,
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    #[serde(default)]
    pub whole_word: Option<bool>,
    #[serde(default)]
    pub include_ignored: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub enum SplitMode {
    /// Clone the current pane.
    #[default]
    ClonePane,
    /// Create an empty new pane.
    EmptyPane,
    /// Move the item into a new pane. This will map to nop if only one pane exists.
    MovePane,
}

macro_rules! split_structs {
    ($($name:ident => $doc:literal),* $(,)?) => {
        $(
            #[doc = $doc]
            #[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
            #[action(namespace = pane)]
            #[serde(deny_unknown_fields, default)]
            pub struct $name {
                pub mode: SplitMode,
            }
        )*
    };
}

split_structs!(
    SplitLeft => "Splits the pane to the left.",
    SplitRight => "Splits the pane to the right.",
    SplitUp => "Splits the pane upward.",
    SplitDown => "Splits the pane downward.",
    SplitHorizontal => "Splits the pane horizontally.",
    SplitVertical => "Splits the pane vertically."
);

/// Activates the previous item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivatePreviousItem {
    /// Whether to wrap from the first item to the last item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivatePreviousItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}

/// Activates the next item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivateNextItem {
    /// Whether to wrap from the last item to the first item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivateNextItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}

actions!(
    pane,
    [
        /// Activates the last item in the pane.
        ActivateLastItem,
        /// Switches to the alternate file.
        AlternateFile,
        /// Navigates back in history.
        GoBack,
        /// Navigates forward in history.
        GoForward,
        /// Navigates back in the tag stack.
        GoToOlderTag,
        /// Navigates forward in the tag stack.
        GoToNewerTag,
        /// Joins this pane into the next pane.
        JoinIntoNext,
        /// Joins all panes into one.
        JoinAll,
        /// Reopens the most recently closed item.
        ReopenClosedItem,
        /// Splits the pane to the left, moving the current item.
        SplitAndMoveLeft,
        /// Splits the pane upward, moving the current item.
        SplitAndMoveUp,
        /// Splits the pane to the right, moving the current item.
        SplitAndMoveRight,
        /// Splits the pane downward, moving the current item.
        SplitAndMoveDown,
        /// Swaps the current item with the one to the left.
        SwapItemLeft,
        /// Swaps the current item with the one to the right.
        SwapItemRight,
        /// Toggles preview mode for the current tab.
        TogglePreviewTab,
        /// Toggles pin status for the current tab.
        TogglePinTab,
        /// Unpins all tabs in the pane.
        UnpinAllTabs,
    ]
);

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

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        next_timestamp: Arc<AtomicUsize>,
        can_drop_predicate: Option<Arc<dyn Fn(&dyn Any, &mut Window, &mut App) -> bool + 'static>>,
        double_click_dispatch_action: Box<dyn Action>,
        use_max_tabs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let max_tabs = if use_max_tabs {
            WorkspaceSettings::get_global(cx).max_tabs
        } else {
            None
        };

        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Pane::focus_in),
            cx.on_focus_in(&focus_handle, window, Pane::focus_in),
            cx.on_focus_out(&focus_handle, window, Pane::focus_out),
            cx.observe_global_in::<SettingsStore>(window, Self::settings_changed),
            cx.subscribe(&project, Self::project_events),
        ];

        let handle = cx.entity().downgrade();

        Self {
            alternate_file_items: (None, None),
            focus_handle,
            items: Vec::new(),
            activation_history: Vec::new(),
            next_activation_timestamp: next_timestamp.clone(),
            was_focused: false,
            zoomed: false,
            active_item_index: 0,
            preview_item_id: None,
            max_tabs,
            use_max_tabs,
            last_focus_handle_by_item: Default::default(),
            nav_history: NavHistory::new(handle, next_timestamp),
            toolbar: cx.new(|_| Toolbar::new()),
            tab_bar_scroll_handle: ScrollHandle::new(),
            suppress_scroll: false,
            drag_split_direction: None,
            drag_swap_target: false,
            drag_tab_target: false,
            drag_tab_insertion_target: None,
            workspace,
            project: project.downgrade(),
            can_drop_predicate,
            can_split_predicate: None,
            can_toggle_zoom: true,
            should_display_tab_bar: Rc::new(|_, cx| TabBarSettings::get_global(cx).show),
            should_display_welcome_page: false,
            render_tab_bar_buttons: Rc::new(pane_render::default_render_tab_bar_buttons),
            render_tab_bar: Rc::new(Self::render_tab_bar),
            show_tab_bar_buttons: TabBarSettings::get_global(cx).show_tab_bar_buttons,
            display_nav_history_buttons: Some(
                TabBarSettings::get_global(cx).show_nav_history_buttons,
            ),
            _subscriptions: subscriptions,
            double_click_dispatch_action,
            save_modals_spawned: HashSet::default(),
            close_pane_if_empty: true,
            split_item_context_menu_handle: Default::default(),
            new_item_context_menu_handle: Default::default(),
            pinned_tab_count: 0,
            diagnostics: Default::default(),
            zoom_out_on_close: true,
            focus_follows_mouse: WorkspaceSettings::get_global(cx).focus_follows_mouse,
            diagnostic_summary_update: Task::ready(()),
            project_item_restoration_data: HashMap::default(),
            welcome_page: None,
            in_center_group: false,
            reserve_traffic_light_space: false,
            pane_kind: PaneKind::Tabs,
            visible: true,
            preferred_horizontal_split_size: None,
        }
    }

    fn alternate_file(&mut self, _: &AlternateFile, window: &mut Window, cx: &mut Context<Pane>) {
        let (_, alternative) = &self.alternate_file_items;
        if let Some(alternative) = alternative {
            let existing = self
                .items()
                .find_position(|item| item.item_id() == alternative.id());
            if let Some((ix, _)) = existing {
                self.activate_item(ix, true, true, window, cx);
            } else if let Some(upgraded) = alternative.upgrade() {
                self.add_item(upgraded, true, true, None, window, cx);
            }
        }
    }

    pub fn track_alternate_file_items(&mut self) {
        if let Some(item) = self.active_item().map(|item| item.downgrade_item()) {
            let (current, _) = &self.alternate_file_items;
            match current {
                Some(current) => {
                    if current.id() != item.id() {
                        self.alternate_file_items =
                            (Some(item), self.alternate_file_items.0.take());
                    }
                }
                None => {
                    self.alternate_file_items = (Some(item), None);
                }
            }
        }
    }

    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        // We not only check whether our focus handle contains focus, but also
        // whether the active item might have focus, because we might have just activated an item
        // that hasn't rendered yet.
        // Before the next render, we might transfer focus
        // to the item, and `focus_handle.contains_focus` returns false because the `active_item`
        // is not hooked up to us in the dispatch tree.
        self.focus_handle.contains_focused(window, cx)
            || self
                .active_item()
                .is_some_and(|item| item.item_focus_handle(cx).contains_focused(window, cx))
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.was_focused {
            self.was_focused = true;
            self.update_history(self.active_item_index);
            if !self.suppress_scroll && self.items.get(self.active_item_index).is_some() {
                self.update_active_tab(self.active_item_index);
            }
            cx.emit(Event::Focus);
            cx.notify();
        }

        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.focus_changed(true, window, cx);
        });

        if let Some(active_item) = self.active_item() {
            if self.focus_handle.is_focused(window) {
                // Schedule a redraw next frame, so that the focus changes below take effect
                cx.on_next_frame(window, |_, _, cx| {
                    cx.notify();
                });

                // Pane was focused directly. We need to either focus a view inside the active item,
                // or focus the active item itself
                if let Some(weak_last_focus_handle) =
                    self.last_focus_handle_by_item.get(&active_item.item_id())
                    && let Some(focus_handle) = weak_last_focus_handle.upgrade()
                {
                    focus_handle.focus(window, cx);
                    return;
                }

                active_item.item_focus_handle(cx).focus(window, cx);
            } else if let Some(focused) = window.focused(cx)
                && !self.context_menu_focused(window, cx)
            {
                self.last_focus_handle_by_item
                    .insert(active_item.item_id(), focused.downgrade());
            }
        } else if self.should_display_welcome_page
            && let Some(welcome_page) = self.welcome_page.as_ref()
        {
            if self.focus_handle.is_focused(window) {
                welcome_page.read(cx).focus_handle(cx).focus(window, cx);
            }
        }
    }

    pub fn context_menu_focused(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.new_item_context_menu_handle.is_focused(window, cx)
            || self.split_item_context_menu_handle.is_focused(window, cx)
    }

    fn focus_out(&mut self, _event: FocusOutEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.was_focused = false;
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.focus_changed(false, window, cx);
        });

        cx.notify();
    }

    fn project_events(
        &mut self,
        _project: Entity<Project>,
        event: &project::Event,
        cx: &mut Context<Self>,
    ) {
        match event {
            project::Event::DiskBasedDiagnosticsFinished { .. }
            | project::Event::DiagnosticsUpdated { .. } => {
                if ItemSettings::get_global(cx).show_diagnostics != ShowDiagnostics::Off {
                    self.diagnostic_summary_update = cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                        this.update(cx, |this, cx| {
                            this.update_diagnostics(cx);
                            cx.notify();
                        })
                        .log_err();
                    });
                }
            }
            _ => {}
        }
    }

    fn update_diagnostics(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.upgrade() else {
            return;
        };
        let show_diagnostics = ItemSettings::get_global(cx).show_diagnostics;
        self.diagnostics = if show_diagnostics != ShowDiagnostics::Off {
            project
                .read(cx)
                .diagnostic_summaries(false, cx)
                .filter_map(|(project_path, _, diagnostic_summary)| {
                    if diagnostic_summary.error_count > 0 {
                        Some((project_path, DiagnosticSeverity::ERROR))
                    } else if diagnostic_summary.warning_count > 0
                        && show_diagnostics != ShowDiagnostics::Errors
                    {
                        Some((project_path, DiagnosticSeverity::WARNING))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            HashMap::default()
        }
    }

    fn settings_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_bar_settings = TabBarSettings::get_global(cx);

        if let Some(display_nav_history_buttons) = self.display_nav_history_buttons.as_mut() {
            *display_nav_history_buttons = tab_bar_settings.show_nav_history_buttons;
        }

        self.show_tab_bar_buttons = tab_bar_settings.show_tab_bar_buttons;

        if !PreviewTabsSettings::get_global(cx).enabled {
            self.preview_item_id = None;
            self.nav_history.0.lock().preview_item_id = None;
        }

        let workspace_settings = WorkspaceSettings::get_global(cx);

        self.focus_follows_mouse = workspace_settings.focus_follows_mouse;

        let new_max_tabs = workspace_settings.max_tabs;

        if self.use_max_tabs && new_max_tabs != self.max_tabs {
            self.max_tabs = new_max_tabs;
            self.close_items_on_settings_change(window, cx);
        }

        self.update_diagnostics(cx);
        cx.notify();
    }

    pub fn active_item_index(&self) -> usize {
        self.active_item_index
    }

    pub fn is_active_item_pinned(&self) -> bool {
        self.is_tab_pinned(self.active_item_index)
    }

    pub fn activation_history(&self) -> &[ActivationHistoryEntry] {
        &self.activation_history
    }

    pub fn set_should_display_tab_bar<F>(&mut self, should_display_tab_bar: F)
    where
        F: 'static + Fn(&Window, &mut Context<Pane>) -> bool,
    {
        self.should_display_tab_bar = Rc::new(should_display_tab_bar);
    }

    pub fn set_should_display_welcome_page(&mut self, should_display_welcome_page: bool) {
        self.should_display_welcome_page = should_display_welcome_page;
    }

    pub fn set_pane_kind(&mut self, pane_kind: PaneKind, cx: &mut Context<Self>) {
        self.pane_kind = pane_kind;
        cx.notify();
    }

    pub fn pane_kind(&self) -> PaneKind {
        self.pane_kind
    }

    pub fn is_tabbed(&self) -> bool {
        self.pane_kind.is_tabbed()
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible != visible {
            self.visible = visible;
            cx.notify();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn preferred_horizontal_split_size(&self) -> Option<Pixels> {
        self.preferred_horizontal_split_size
    }

    pub fn remember_horizontal_split_size(&mut self, size: Pixels) {
        if size > Pixels::ZERO {
            self.preferred_horizontal_split_size = Some(size);
        }
    }

    pub fn set_reserve_traffic_light_space(
        &mut self,
        reserve_traffic_light_space: bool,
        cx: &mut Context<Self>,
    ) {
        if self.reserve_traffic_light_space != reserve_traffic_light_space {
            self.reserve_traffic_light_space = reserve_traffic_light_space;
            cx.notify();
        }
    }

    pub fn should_reserve_traffic_light_space(&self, window: &Window, cx: &App) -> bool {
        if !cfg!(target_os = "macos")
            || window.is_fullscreen()
            || !(self.reserve_traffic_light_space || self.zoomed)
            || Self::titlebar_visible(cx)
        {
            return false;
        }

        !self.left_sidebar_visible(cx)
    }

    fn titlebar_visible(_cx: &App) -> bool {
        false
    }

    fn left_sidebar_visible(&self, cx: &App) -> bool {
        self.workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).multi_workspace().cloned())
            .and_then(|multi_workspace| multi_workspace.upgrade())
            .is_some_and(|multi_workspace| {
                let sidebar = multi_workspace.read(cx).sidebar_render_state(cx);
                sidebar.open && sidebar.side == SidebarSide::Left
            })
    }

    pub fn set_can_split(
        &mut self,
        can_split_predicate: Option<
            Arc<dyn Fn(&mut Self, &dyn Any, &mut Window, &mut Context<Self>) -> bool + 'static>,
        >,
    ) {
        self.can_split_predicate = can_split_predicate;
    }

    pub fn set_can_toggle_zoom(&mut self, can_toggle_zoom: bool, cx: &mut Context<Self>) {
        self.can_toggle_zoom = can_toggle_zoom;
        cx.notify();
    }

    pub fn set_close_pane_if_empty(&mut self, close_pane_if_empty: bool, cx: &mut Context<Self>) {
        self.close_pane_if_empty = close_pane_if_empty;
        cx.notify();
    }

    pub fn set_can_navigate(&mut self, can_navigate: bool, cx: &mut Context<Self>) {
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_can_navigate(can_navigate, cx);
        });
        cx.notify();
    }

    pub fn set_render_tab_bar<F>(&mut self, cx: &mut Context<Self>, render: F)
    where
        F: 'static + Fn(&mut Pane, &mut Window, &mut Context<Pane>) -> AnyElement,
    {
        self.render_tab_bar = Rc::new(render);
        cx.notify();
    }

    pub fn set_render_tab_bar_buttons<F>(&mut self, cx: &mut Context<Self>, render: F)
    where
        F: 'static
            + Fn(
                &mut Pane,
                &mut Window,
                &mut Context<Pane>,
            ) -> (Option<AnyElement>, Option<AnyElement>),
    {
        self.render_tab_bar_buttons = Rc::new(render);
        cx.notify();
    }

    pub fn nav_history_for_item<T: Item>(&self, item: &Entity<T>) -> ItemNavHistory {
        self.nav_history.for_item(Arc::new(item.downgrade()))
    }

    pub fn nav_history(&self) -> &NavHistory {
        &self.nav_history
    }

    pub fn nav_history_mut(&mut self) -> &mut NavHistory {
        &mut self.nav_history
    }

    pub fn fork_nav_history(&self) -> NavHistory {
        self.nav_history.fork()
    }

    pub fn set_nav_history(&mut self, history: NavHistory, cx: &Context<Self>) {
        self.nav_history = history;
        self.nav_history.set_pane(cx.entity().downgrade());
    }

    pub fn disable_history(&mut self) {
        self.nav_history.disable();
    }

    pub fn enable_history(&mut self) {
        self.nav_history.enable();
    }

    pub fn can_navigate_backward(&self) -> bool {
        !self.nav_history.0.lock().backward_stack.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        !self.nav_history.0.lock().forward_stack.is_empty()
    }

    pub fn navigate_backward(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.go_back(pane, window, cx).detach_and_log_err(cx)
                })
            })
        }
    }

    fn navigate_forward(&mut self, _: &GoForward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .go_forward(pane, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    pub fn go_to_older_tag(
        &mut self,
        _: &GoToOlderTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .navigate_tag_history(pane, TagNavigationMode::Older, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    pub fn go_to_newer_tag(
        &mut self,
        _: &GoToNewerTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let pane = cx.entity().downgrade();
            window.defer(cx, move |window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace
                        .navigate_tag_history(pane, TagNavigationMode::Newer, window, cx)
                        .detach_and_log_err(cx)
                })
            })
        }
    }

    fn history_updated(&mut self, cx: &mut Context<Self>) {
        self.toolbar.update(cx, |_, cx| cx.notify());
    }

    pub fn set_pinned_count(&mut self, count: usize) {
        self.pinned_tab_count = count;
    }

    pub fn pinned_count(&self) -> usize {
        self.pinned_tab_count
    }

    pub async fn save_item(
        project: Entity<Project>,
        pane: &WeakEntity<Pane>,
        item: &dyn ItemHandle,
        save_intent: SaveIntent,
        cx: &mut AsyncWindowContext,
    ) -> Result<bool> {
        const CONFLICT_MESSAGE: &str = "This file has changed on disk since you started editing it. Do you want to overwrite it?";

        const DELETED_MESSAGE: &str = "This file has been deleted on disk since you started editing it. Do you want to recreate it?";

        let path_style = project.read_with(cx, |project, cx| project.path_style(cx));
        if save_intent == SaveIntent::Skip {
            let is_saveable_singleton = cx.update(|_window, cx| {
                item.can_save(cx) && item.buffer_kind(cx) == ItemBufferKind::Singleton
            })?;
            if is_saveable_singleton {
                pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                    .await
                    .log_err();
            }
            return Ok(true);
        };
        let Some(item_ix) = pane
            .read_with(cx, |pane, _| pane.index_for_item(item))
            .ok()
            .flatten()
        else {
            return Ok(true);
        };

        let (
            mut has_conflict,
            mut is_dirty,
            mut can_save,
            can_save_as,
            is_singleton,
            has_deleted_file,
        ) = cx.update(|_window, cx| {
            (
                item.has_conflict(cx),
                item.is_dirty(cx),
                item.can_save(cx),
                item.can_save_as(cx),
                item.buffer_kind(cx) == ItemBufferKind::Singleton,
                item.has_deleted_file(cx),
            )
        })?;

        // when saving a single buffer, we ignore whether or not it's dirty.
        if save_intent == SaveIntent::Save
            || save_intent == SaveIntent::FormatAndSave
            || save_intent == SaveIntent::SaveWithoutFormat
        {
            is_dirty = true;
        }

        if save_intent == SaveIntent::SaveAs {
            is_dirty = true;
            has_conflict = false;
            can_save = false;
        }

        if save_intent == SaveIntent::Overwrite {
            has_conflict = false;
        }

        let should_format = save_intent != SaveIntent::SaveWithoutFormat;
        let force_format = save_intent == SaveIntent::FormatAndSave;

        if has_conflict && can_save {
            if has_deleted_file && is_singleton {
                let answer = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    window.prompt(
                        PromptLevel::Warning,
                        DELETED_MESSAGE,
                        None,
                        &["Save", "Close", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => {
                        pane.update_in(cx, |_, window, cx| {
                            item.save(
                                SaveOptions {
                                    format: should_format,
                                    force_format,
                                    autosave: false,
                                },
                                project,
                                window,
                                cx,
                            )
                        })?
                        .await?
                    }
                    Ok(1) => {
                        pane.update_in(cx, |pane, window, cx| {
                            pane.remove_item(item.item_id(), false, true, window, cx)
                        })?;
                    }
                    _ => return Ok(false),
                }
                return Ok(true);
            } else {
                let answer = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    window.prompt(
                        PromptLevel::Warning,
                        CONFLICT_MESSAGE,
                        None,
                        &["Overwrite", "Discard", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => {
                        pane.update_in(cx, |_, window, cx| {
                            item.save(
                                SaveOptions {
                                    format: should_format,
                                    force_format,
                                    autosave: false,
                                },
                                project,
                                window,
                                cx,
                            )
                        })?
                        .await?
                    }
                    Ok(1) => {
                        pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                            .await?
                    }
                    _ => return Ok(false),
                }
            }
        } else if is_dirty && (can_save || can_save_as) {
            if save_intent == SaveIntent::Close {
                let will_autosave = cx.update(|_window, cx| {
                    item.can_autosave(cx)
                        && item.workspace_settings(cx).autosave.should_save_on_close()
                })?;
                if !will_autosave {
                    let item_id = item.item_id();
                    let answer_task = pane.update_in(cx, |pane, window, cx| {
                        if pane.save_modals_spawned.insert(item_id) {
                            pane.activate_item(item_ix, true, true, window, cx);
                            let prompt = dirty_message_for(item.project_path(cx), path_style);
                            Some(window.prompt(
                                PromptLevel::Warning,
                                &prompt,
                                None,
                                &["Save", "Don't Save", "Cancel"],
                                cx,
                            ))
                        } else {
                            None
                        }
                    })?;
                    if let Some(answer_task) = answer_task {
                        let answer = answer_task.await;
                        pane.update(cx, |pane, _| {
                            if !pane.save_modals_spawned.remove(&item_id) {
                                debug_panic!(
                                    "save modal was not present in spawned modals after awaiting for its answer"
                                )
                            }
                        })?;
                        match answer {
                            Ok(0) => {}
                            Ok(1) => {
                                // Don't save this file - reload from disk to discard changes
                                pane.update_in(cx, |pane, _, cx| {
                                    if pane.is_tab_pinned(item_ix) && !item.can_save(cx) {
                                        pane.pinned_tab_count -= 1;
                                    }
                                })
                                .log_err();
                                if can_save && is_singleton {
                                    pane.update_in(cx, |_, window, cx| {
                                        item.reload(project.clone(), window, cx)
                                    })?
                                    .await
                                    .log_err();
                                }
                                return Ok(true);
                            }
                            _ => return Ok(false), // Cancel
                        }
                    } else {
                        return Ok(false);
                    }
                }
            }

            if can_save {
                pane.update_in(cx, |pane, window, cx| {
                    pane.unpreview_item_if_preview(item.item_id());
                    item.save(
                        SaveOptions {
                            format: should_format,
                            force_format,
                            autosave: false,
                        },
                        project,
                        window,
                        cx,
                    )
                })?
                .await?;
            } else if can_save_as && is_singleton {
                let suggested_name =
                    cx.update(|_window, cx| item.suggested_filename(cx).to_string())?;
                let new_path = pane.update_in(cx, |pane, window, cx| {
                    pane.activate_item(item_ix, true, true, window, cx);
                    pane.workspace.update(cx, |workspace, cx| {
                        let lister = if workspace.project().read(cx).is_local() {
                            DirectoryLister::Local(
                                workspace.project().clone(),
                                workspace.app_state().fs.clone(),
                            )
                        } else {
                            DirectoryLister::Project(workspace.project().clone())
                        };
                        workspace.prompt_for_new_path(lister, Some(suggested_name), window, cx)
                    })
                })??;
                let Some(new_path) = new_path.await.ok().flatten().into_iter().flatten().next()
                else {
                    return Ok(false);
                };

                let project_path = pane
                    .update(cx, |pane, cx| {
                        pane.project
                            .update(cx, |project, cx| {
                                project.find_or_create_worktree(new_path, true, cx)
                            })
                            .ok()
                    })
                    .ok()
                    .flatten();
                let save_task = if let Some(project_path) = project_path {
                    let (worktree, path) = project_path.await?;
                    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
                    let new_path = ProjectPath { worktree_id, path };

                    pane.update_in(cx, |pane, window, cx| {
                        if let Some(item) = pane.item_for_path(new_path.clone(), cx) {
                            pane.remove_item(item.item_id(), false, false, window, cx);
                        }

                        item.save_as(project.clone(), new_path, window, cx)
                    })?
                } else {
                    return Ok(false);
                };

                save_task.await?;
                if should_format {
                    pane.update_in(cx, |pane, window, cx| {
                        pane.unpreview_item_if_preview(item.item_id());
                        item.save(
                            SaveOptions {
                                format: true,
                                autosave: false,
                                force_format,
                            },
                            project,
                            window,
                            cx,
                        )
                    })?
                    .await?;
                }
                return Ok(true);
            }
        }

        pane.update(cx, |_, cx| {
            cx.emit(Event::UserSavedItem {
                item: item.downgrade_item(),
                save_intent,
            });
            true
        })
    }

    pub fn autosave_item(
        item: &dyn ItemHandle,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let format = !matches!(
            item.workspace_settings(cx).autosave,
            AutosaveSetting::AfterDelay { .. }
        );
        if item.can_autosave(cx) {
            item.save(
                SaveOptions {
                    format,
                    force_format: false,
                    autosave: true,
                },
                project,
                window,
                cx,
            )
        } else {
            Task::ready(Ok(()))
        }
    }

    pub fn focus_active_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_item) = self.active_item() {
            let focus_handle = active_item.item_focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub fn split(
        &mut self,
        direction: SplitDirection,
        mode: SplitMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.items.len() <= 1 && mode == SplitMode::MovePane {
            // MovePane with only one pane present behaves like a SplitEmpty in the opposite direction
            let active_item = self.active_item();
            cx.emit(Event::Split {
                direction: direction.opposite(),
                mode: SplitMode::EmptyPane,
            });
            // ensure that we focus the moved pane
            // in this case we know that the window is the same as the active_item
            if let Some(active_item) = active_item {
                cx.defer_in(window, move |_, window, cx| {
                    let focus_handle = active_item.item_focus_handle(cx);
                    window.focus(&focus_handle, cx);
                });
            }
        } else {
            cx.emit(Event::Split { direction, mode });
        }
    }

    pub fn toolbar(&self) -> &Entity<Toolbar> {
        &self.toolbar
    }

    pub fn handle_deleted_project_item(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> Option<()> {
        let item_id = self.items().find_map(|item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && item.project_entry_ids(cx).as_slice() == [entry_id]
            {
                Some(item.item_id())
            } else {
                None
            }
        })?;

        self.remove_item(item_id, false, true, window, cx);
        self.nav_history.remove_item(item_id);

        Some(())
    }

    fn update_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_item = self
            .items
            .get(self.active_item_index)
            .map(|item| item.as_ref());
        self.toolbar.update(cx, |toolbar, cx| {
            toolbar.set_active_item(active_item, window, cx);
        });
    }

    fn update_status_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let pane = cx.entity();

        window.defer(cx, move |window, cx| {
            let Ok(status_bar) =
                workspace.read_with(cx, |workspace, _| workspace.status_bar.clone())
            else {
                return;
            };

            status_bar.update(cx, move |status_bar, cx| {
                status_bar.set_active_pane(&pane, window, cx);
            });
        });
    }

    fn entry_abs_path(&self, entry: ProjectEntryId, cx: &App) -> Option<PathBuf> {
        let worktree = self
            .workspace
            .upgrade()?
            .read(cx)
            .project()
            .read(cx)
            .worktree_for_entry(entry, cx)?
            .read(cx);
        let entry = worktree.entry_for_id(entry)?;
        Some(match &entry.canonical_path {
            Some(canonical_path) => canonical_path.to_path_buf(),
            None => worktree.absolutize(&entry.path),
        })
    }

    pub fn icon_color(selected: bool) -> Color {
        if selected {
            Color::Default
        } else {
            Color::Muted
        }
    }

    pub fn set_zoomed(&mut self, zoomed: bool, cx: &mut Context<Self>) {
        self.zoomed = zoomed;
        cx.notify();
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoomed
    }
}

pub(crate) fn render_toggle_zoom_button(pane: &Pane, cx: &mut Context<Pane>) -> IconButton {
    let zoomed = pane.is_zoomed();
    IconButton::new("toggle_zoom", IconName::Maximize)
        .icon_size(IconSize::Small)
        .toggle_state(zoomed)
        .selected_icon(IconName::Minimize)
        .on_click(cx.listener(|pane, _, window, cx| {
            pane.toggle_zoom(&crate::ToggleZoom, window, cx);
        }))
        .tooltip(move |_window, cx| {
            Tooltip::for_action(if zoomed { "Zoom Out" } else { "Zoom In" }, &ToggleZoom, cx)
        })
}

fn dirty_message_for(buffer_path: Option<ProjectPath>, path_style: PathStyle) -> String {
    let path = buffer_path
        .as_ref()
        .and_then(|p| {
            let path = p.path.display(path_style);
            if path.is_empty() { None } else { Some(path) }
        })
        .unwrap_or("This buffer".into());
    let path = truncate_and_remove_front(&path, 80);
    format!("{path} contains unsaved edits. Do you want to save it?")
}

pub fn tab_details(items: &[Box<dyn ItemHandle>], _window: &Window, cx: &App) -> Vec<usize> {
    util::disambiguate::compute_disambiguation_details(items, |item, detail| {
        item.tab_content_text(detail, cx)
    })
}

pub fn render_item_indicator(item: Box<dyn ItemHandle>, cx: &App) -> Option<Indicator> {
    maybe!({
        let indicator_color = match (item.has_conflict(cx), item.is_dirty(cx)) {
            (true, _) => Color::Warning,
            (_, true) => Color::Accent,
            (false, false) => return None,
        };

        Some(Indicator::dot().color(indicator_color))
    })
}

impl Render for DraggedTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = ThemeSettings::get_global(cx).ui_font.clone();
        let label = self.item.tab_content(
            TabContentParams {
                detail: Some(self.detail),
                selected: false,
                preview: false,
                deemphasized: false,
                max_title_len: None,
                truncate_title_middle: false,
            },
            window,
            cx,
        );
        Tab::new("")
            .toggle_state(self.is_active)
            .child(label)
            .render(window, cx)
            .font(ui_font)
    }
}

impl Render for DraggedPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .h(Tab::container_height(cx))
            .flex()
            .items_center()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_bar_background)
            .child(Label::new("Pane").color(Color::Muted))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, iter::zip, num::NonZero, rc::Rc};

    use super::*;
    use crate::{
        Member,
        item::test::{TestItem, TestProjectItem},
    };
    use gpui::{
        AppContext, Axis, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
        TestAppContext, VisualTestContext, size,
    };
    use project::FakeFs;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::TryFutureExt;

    // drop_call_count is a Cell here because `handle_drop` takes &self, not &mut self.
    struct CustomDropHandlingItem {
        focus_handle: gpui::FocusHandle,
        drop_call_count: Cell<usize>,
    }

    impl CustomDropHandlingItem {
        fn new(cx: &mut Context<Self>) -> Self {
            Self {
                focus_handle: cx.focus_handle(),
                drop_call_count: Cell::new(0),
            }
        }

        fn drop_call_count(&self) -> usize {
            self.drop_call_count.get()
        }
    }

    impl EventEmitter<()> for CustomDropHandlingItem {}

    impl Focusable for CustomDropHandlingItem {
        fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for CustomDropHandlingItem {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            gpui::Empty
        }
    }

    impl Item for CustomDropHandlingItem {
        type Event = ();

        fn tab_content_text(&self, _detail: usize, _cx: &App) -> gpui::SharedString {
            "custom_drop_handling_item".into()
        }

        fn handle_drop(
            &self,
            _active_pane: &Pane,
            dropped: &dyn std::any::Any,
            _window: &mut Window,
            _cx: &mut App,
        ) -> bool {
            let is_dragged_tab = dropped.downcast_ref::<DraggedTab>().is_some();
            if is_dragged_tab {
                self.drop_call_count.set(self.drop_call_count.get() + 1);
            }
            is_dragged_tab
        }
    }

    #[gpui::test]
    async fn test_add_item_capped_to_max_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        for i in 0..7 {
            add_labeled_item(&pane, format!("{}", i).as_str(), false, cx);
        }

        set_max_tabs(cx, Some(5));
        add_labeled_item(&pane, "7", false, cx);
        // Remove items to respect the max tab cap.
        assert_item_labels(&pane, ["3", "4", "5", "6", "7*"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(0, false, false, window, cx);
        });
        add_labeled_item(&pane, "X", false, cx);
        // Respect activation order.
        assert_item_labels(&pane, ["3", "X*", "5", "6", "7"], cx);

        for i in 0..7 {
            add_labeled_item(&pane, format!("D{}", i).as_str(), true, cx);
        }
        // Keeps dirty items, even over max tab cap.
        assert_item_labels(
            &pane,
            ["D0^", "D1^", "D2^", "D3^", "D4^", "D5^", "D6*^"],
            cx,
        );

        set_max_tabs(cx, None);
        for i in 0..7 {
            add_labeled_item(&pane, format!("N{}", i).as_str(), false, cx);
        }
        // No cap when max tabs is None.
        assert_item_labels(
            &pane,
            [
                "D0^", "D1^", "D2^", "D3^", "D4^", "D5^", "D6^", "N0", "N1", "N2", "N3", "N4",
                "N5", "N6*",
            ],
            cx,
        );
    }

    #[gpui::test]
    async fn test_reduce_max_tabs_closes_existing_items(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        let item_d = add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        add_labeled_item(&pane, "Settings", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D", "E", "Settings*"], cx);

        set_max_tabs(cx, Some(5));
        assert_item_labels(&pane, ["B", "C", "D", "E", "Settings*"], cx);

        set_max_tabs(cx, Some(4));
        assert_item_labels(&pane, ["C", "D", "E", "Settings*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C!", "D!", "E", "Settings*"], cx);

        set_max_tabs(cx, Some(2));
        assert_item_labels(&pane, ["C!", "D!", "Settings*"], cx);
    }

    #[gpui::test]
    async fn test_allow_pinning_dirty_item_at_max_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(1));
        let item_a = add_labeled_item(&pane, "A", true, cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*^!"], cx);
    }

    #[gpui::test]
    async fn test_allow_pinning_non_dirty_item_at_max_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(1));
        let item_a = add_labeled_item(&pane, "A", false, cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*!"], cx);
    }

    #[gpui::test]
    async fn test_pin_tabs_incrementally_at_max_capacity(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(3));

        let item_a = add_labeled_item(&pane, "A", false, cx);
        assert_item_labels(&pane, ["A*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*!"], cx);

        let item_b = add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A!", "B*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*!"], cx);

        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);
    }

    #[gpui::test]
    async fn test_pin_tabs_left_to_right_after_opening_at_max_capacity(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(3));

        let item_a = add_labeled_item(&pane, "A", false, cx);
        assert_item_labels(&pane, ["A*"], cx);

        let item_b = add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A", "B*"], cx);

        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);
    }

    #[gpui::test]
    async fn test_pin_tabs_right_to_left_after_opening_at_max_capacity(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(3));

        let item_a = add_labeled_item(&pane, "A", false, cx);
        assert_item_labels(&pane, ["A*"], cx);

        let item_b = add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A", "B*"], cx);

        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C*!", "A", "B"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C*!", "B!", "A"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C*!", "B!", "A!"], cx);
    }

    #[gpui::test]
    async fn test_pinned_tabs_never_closed_at_max_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        let item_b = add_labeled_item(&pane, "B", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C", "D", "E*"], cx);

        set_max_tabs(cx, Some(3));
        add_labeled_item(&pane, "F", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "F*"], cx);

        add_labeled_item(&pane, "G", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "G*"], cx);

        add_labeled_item(&pane, "H", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "H*"], cx);
    }

    #[gpui::test]
    async fn test_always_allows_one_unpinned_item_over_max_tabs_regardless_of_pinned_count(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(3));

        let item_a = add_labeled_item(&pane, "A", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        let item_b = add_labeled_item(&pane, "B", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        let item_c = add_labeled_item(&pane, "C", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);

        let item_d = add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C!", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C!", "D*!"], cx);

        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C!", "D!", "E*"], cx);

        add_labeled_item(&pane, "F", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C!", "D!", "F*"], cx);
    }

    #[gpui::test]
    async fn test_can_open_one_item_when_all_tabs_are_dirty_at_max(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_max_tabs(cx, Some(3));

        add_labeled_item(&pane, "A", true, cx);
        assert_item_labels(&pane, ["A*^"], cx);

        add_labeled_item(&pane, "B", true, cx);
        assert_item_labels(&pane, ["A^", "B*^"], cx);

        add_labeled_item(&pane, "C", true, cx);
        assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A^", "B^", "C^", "D*"], cx);

        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A^", "B^", "C^", "E*"], cx);

        add_labeled_item(&pane, "F", false, cx);
        assert_item_labels(&pane, ["A^", "B^", "C^", "F*"], cx);

        add_labeled_item(&pane, "G", true, cx);
        assert_item_labels(&pane, ["A^", "B^", "C^", "G*^"], cx);
    }

    #[gpui::test]
    async fn test_toggle_pin_tab(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_labeled_items(&pane, ["A", "B*", "C"], cx);
        assert_item_labels(&pane, ["A", "B*", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.toggle_pin_tab(&TogglePinTab, window, cx);
        });
        assert_item_labels(&pane, ["B*!", "A", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.toggle_pin_tab(&TogglePinTab, window, cx);
        });
        assert_item_labels(&pane, ["B*", "A", "C"], cx);
    }

    #[gpui::test]
    async fn test_unpin_all_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Unpin all, in an empty pane
        pane.update_in(cx, |pane, window, cx| {
            pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
        });

        assert_item_labels(&pane, [], cx);

        let item_a = add_labeled_item(&pane, "A", false, cx);
        let item_b = add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Unpin all, when no tabs are pinned
        pane.update_in(cx, |pane, window, cx| {
            pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
        });

        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Pin inactive tabs only
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
        });

        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Pin all tabs
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B!", "C*!"], cx);

        // Activate middle tab
        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(1, false, false, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*!", "C!"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.unpin_all_tabs(&UnpinAllTabs, window, cx);
        });

        // Order has not changed
        assert_item_labels(&pane, ["A", "B*", "C"], cx);
    }

    #[gpui::test]
    async fn test_separate_pinned_row_disabled_by_default(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);

        // Pin one tab
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B", "C*"], cx);

        // Verify setting is disabled by default
        let is_separate_row_enabled = pane.read_with(cx, |_, cx| {
            TabBarSettings::get_global(cx).show_pinned_tabs_in_separate_row
        });
        assert!(
            !is_separate_row_enabled,
            "Separate pinned row should be disabled by default"
        );

        // Verify pinned_tabs_row element does NOT exist (single row layout)
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_none(),
            "pinned_tabs_row should not exist when setting is disabled"
        );
    }

    #[gpui::test]
    async fn test_separate_pinned_row_two_rows_when_both_tab_types_exist(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Enable separate row setting
        set_pinned_tabs_separate_row(cx, true);

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);

        // Pin one tab - now we have both pinned and unpinned tabs
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B", "C*"], cx);

        // Verify pinned_tabs_row element exists (two row layout)
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_some(),
            "pinned_tabs_row should exist when setting is enabled and both tab types exist"
        );
    }

    #[gpui::test]
    async fn test_separate_pinned_row_single_row_when_only_pinned_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Enable separate row setting
        set_pinned_tabs_separate_row(cx, true);

        let item_a = add_labeled_item(&pane, "A", false, cx);
        let item_b = add_labeled_item(&pane, "B", false, cx);

        // Pin all tabs - only pinned tabs exist
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*!"], cx);

        // Verify pinned_tabs_row does NOT exist (single row layout for pinned-only)
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_none(),
            "pinned_tabs_row should not exist when only pinned tabs exist (uses single row)"
        );
    }

    #[gpui::test]
    async fn test_separate_pinned_row_single_row_when_only_unpinned_tabs(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Enable separate row setting
        set_pinned_tabs_separate_row(cx, true);

        // Add only unpinned tabs
        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Verify pinned_tabs_row does NOT exist (single row layout for unpinned-only)
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_none(),
            "pinned_tabs_row should not exist when only unpinned tabs exist (uses single row)"
        );
    }

    #[gpui::test]
    async fn test_separate_pinned_row_toggles_between_layouts(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);

        // Pin one tab
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });

        // Initially disabled - single row
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_none(),
            "Should be single row when disabled"
        );

        // Enable - two rows
        set_pinned_tabs_separate_row(cx, true);
        cx.run_until_parked();
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_some(),
            "Should be two rows when enabled"
        );

        // Disable again - back to single row
        set_pinned_tabs_separate_row(cx, false);
        cx.run_until_parked();
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_none(),
            "Should be single row when disabled again"
        );
    }

    #[gpui::test]
    async fn test_separate_pinned_row_has_right_border(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Enable separate row setting
        set_pinned_tabs_separate_row(cx, true);

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);

        // Pin one tab - now we have both pinned and unpinned tabs (two-row layout)
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B", "C*"], cx);
        cx.run_until_parked();

        // Verify two-row layout is active
        let pinned_row_bounds = cx.debug_bounds("pinned_tabs_row");
        assert!(
            pinned_row_bounds.is_some(),
            "Two-row layout should be active when both pinned and unpinned tabs exist"
        );

        // Verify pinned_tabs_border element exists (the right border after pinned tabs)
        let border_bounds = cx.debug_bounds("pinned_tabs_border");
        assert!(
            border_bounds.is_some(),
            "pinned_tabs_border should exist in two-row layout to show right border"
        );
    }

    #[gpui::test]
    async fn test_pinning_active_tab_without_position_change_maintains_focus(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A
        let item_a = add_labeled_item(&pane, "A", false, cx);
        assert_item_labels(&pane, ["A*"], cx);

        // Add B
        add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A", "B*"], cx);

        // Activate A again
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.activate_item(ix, true, true, window, cx);
        });
        assert_item_labels(&pane, ["A*", "B"], cx);

        // Pin A - remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*!", "B"], cx);

        // Unpin A - remain active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*", "B"], cx);
    }

    #[gpui::test]
    async fn test_pinning_active_tab_with_position_change_maintains_focus(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C
        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Pin C - moves to pinned area, remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C*!", "A", "B"], cx);

        // Unpin C - moves after pinned area, remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C*", "A", "B"], cx);
    }

    #[gpui::test]
    async fn test_pinning_inactive_tab_without_position_change_preserves_existing_focus(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B
        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A", "B*"], cx);

        // Pin A - already in pinned area, B remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*"], cx);

        // Unpin A - stays in place, B remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A", "B*"], cx);
    }

    #[gpui::test]
    async fn test_pinning_inactive_tab_with_position_change_preserves_existing_focus(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C
        add_labeled_item(&pane, "A", false, cx);
        let item_b = add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // Activate B
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.activate_item(ix, true, true, window, cx);
        });
        assert_item_labels(&pane, ["A", "B*", "C"], cx);

        // Pin C - moves to pinned area, B remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C!", "A", "B*"], cx);

        // Unpin C - moves after pinned area, B remains active
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["C", "A", "B*"], cx);
    }

    #[gpui::test]
    async fn test_handle_tab_drop_respects_is_pane_target(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let source_pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&source_pane, "A", false, cx);
        let item_b = add_labeled_item(&source_pane, "B", false, cx);

        let target_pane = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(source_pane.clone(), SplitDirection::Right, window, cx)
        });

        let custom_item = target_pane.update_in(cx, |pane, window, cx| {
            let custom_item = Box::new(cx.new(CustomDropHandlingItem::new));
            pane.add_item(custom_item.clone(), true, true, None, window, cx);
            custom_item
        });

        let moved_item_id = item_a.item_id();
        let other_item_id = item_b.item_id();
        let custom_item_id = custom_item.item_id();

        let pane_item_ids = |pane: &Entity<Pane>, cx: &mut VisualTestContext| {
            pane.read_with(cx, |pane, _| {
                pane.items().map(|item| item.item_id()).collect::<Vec<_>>()
            })
        };

        let source_before_item_ids = pane_item_ids(&source_pane, cx);
        assert_eq!(source_before_item_ids, vec![moved_item_id, other_item_id]);

        let target_before_item_ids = pane_item_ids(&target_pane, cx);
        assert_eq!(target_before_item_ids, vec![custom_item_id]);

        let dragged_tab = DraggedTab {
            pane: source_pane.clone(),
            item: item_a.boxed_clone(),
            ix: 0,
            detail: 0,
            is_active: true,
        };

        // Dropping item_a onto the target pane itself means the
        // custom item handles the drop and no tab move should occur
        target_pane.update_in(cx, |pane, window, cx| {
            pane.handle_tab_drop(&dragged_tab, pane.active_item_index(), true, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            custom_item.read_with(cx, |item, _| item.drop_call_count()),
            1
        );
        assert_eq!(pane_item_ids(&source_pane, cx), source_before_item_ids);
        assert_eq!(pane_item_ids(&target_pane, cx), target_before_item_ids);

        // Dropping item_a onto the tab target means the custom handler
        // should be skipped and the pane's default tab drop behavior should run.
        target_pane.update_in(cx, |pane, window, cx| {
            pane.handle_tab_drop(&dragged_tab, pane.active_item_index(), false, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            custom_item.read_with(cx, |item, _| item.drop_call_count()),
            1
        );
        assert_eq!(pane_item_ids(&source_pane, cx), vec![other_item_id]);

        let target_item_ids = pane_item_ids(&target_pane, cx);
        assert_eq!(target_item_ids, vec![moved_item_id, custom_item_id]);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_to_split_creates_pane_with_unpinned_tab(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B. Pin B. Activate A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);

        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.activate_item(ix, true, true, window, cx);
        });

        // Drag A to create new split
        pane_a.update_in(cx, |pane, window, cx| {
            pane.drag_split_direction = Some(SplitDirection::Right);

            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, true, window, cx);
        });

        // A should be moved to new pane. B should remain pinned, A should not be pinned
        let (pane_a, pane_b) = workspace.read_with(cx, |workspace, _| {
            let panes = workspace.panes();
            (panes[0].clone(), panes[1].clone())
        });
        assert_item_labels(&pane_a, ["B*!"], cx);
        assert_item_labels(&pane_b, ["A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_to_split_creates_pane_with_pinned_tab(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B. Pin both. Activate A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);

        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.activate_item(ix, true, true, window, cx);
        });
        assert_item_labels(&pane_a, ["A*!", "B!"], cx);

        // Drag A to create new split
        pane_a.update_in(cx, |pane, window, cx| {
            pane.drag_split_direction = Some(SplitDirection::Right);

            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, true, window, cx);
        });

        // A should be moved to new pane. Both A and B should still be pinned
        let (pane_a, pane_b) = workspace.read_with(cx, |workspace, _| {
            let panes = workspace.panes();
            (panes[0].clone(), panes[1].clone())
        });
        assert_item_labels(&pane_a, ["B*!"], cx);
        assert_item_labels(&pane_b, ["A*!"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_into_existing_panes_pinned_region(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A to pane A and pin
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A*!"], cx);

        // Add B to pane B and pin
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        let item_b = add_labeled_item(&pane_b, "B", false, cx);
        pane_b.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_b, ["B*!"], cx);

        // Move A from pane A to pane B's pinned region
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // A should stay pinned
        assert_item_labels(&pane_a, [], cx);
        assert_item_labels(&pane_b, ["A*!", "B!"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_into_existing_panes_unpinned_region(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A to pane A and pin
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A*!"], cx);

        // Create pane B with pinned item B
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        let item_b = add_labeled_item(&pane_b, "B", false, cx);
        assert_item_labels(&pane_b, ["B*"], cx);

        pane_b.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_b, ["B*!"], cx);

        // Move A from pane A to pane B's unpinned region
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // A should become pinned
        assert_item_labels(&pane_a, [], cx);
        assert_item_labels(&pane_b, ["B!", "A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_into_existing_panes_first_position_with_no_pinned_tabs(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A to pane A and pin
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A*!"], cx);

        // Add B to pane B
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        add_labeled_item(&pane_b, "B", false, cx);
        assert_item_labels(&pane_b, ["B*"], cx);

        // Move A from pane A to position 0 in pane B, indicating it should stay pinned
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // A should stay pinned
        assert_item_labels(&pane_a, [], cx);
        assert_item_labels(&pane_b, ["A*!", "B"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_into_existing_pane_at_max_capacity_closes_unpinned_tabs(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        set_max_tabs(cx, Some(2));

        // Add A, B to pane A. Pin both
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B*!"], cx);

        // Add C, D to pane B. Pin both
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        let item_c = add_labeled_item(&pane_b, "C", false, cx);
        let item_d = add_labeled_item(&pane_b, "D", false, cx);
        pane_b.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_d.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_b, ["C!", "D*!"], cx);

        // Add a third unpinned item to pane B (exceeds max tabs), but is allowed,
        // as we allow 1 tab over max if the others are pinned or dirty
        add_labeled_item(&pane_b, "E", false, cx);
        assert_item_labels(&pane_b, ["C!", "D!", "E*"], cx);

        // Drag pinned A from pane A to position 0 in pane B
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // E (unpinned) should be closed, leaving 3 pinned items
        assert_item_labels(&pane_a, ["B*!"], cx);
        assert_item_labels(&pane_b, ["A*!", "C!", "D!"], cx);
    }

    #[gpui::test]
    async fn test_drag_last_pinned_tab_to_same_position_stays_pinned(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A to pane A and pin it
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A*!"], cx);

        // Drag pinned A to position 1 (directly to the right) in the same pane
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // A should still be pinned and active
        assert_item_labels(&pane_a, ["A*!"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_beyond_last_pinned_tab_in_same_pane_stays_pinned(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B to pane A and pin both
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B*!"], cx);

        // Drag pinned A right of B in the same pane
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
        });

        // A stays pinned
        assert_item_labels(&pane_a, ["B!", "A*!"], cx);
    }

    #[gpui::test]
    async fn test_dragging_pinned_tab_onto_unpinned_tab_reduces_unpinned_tab_count(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B to pane A and pin A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        add_labeled_item(&pane_a, "B", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B*"], cx);

        // Drag pinned A on top of B in the same pane, which changes tab order to B, A
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // Neither are pinned
        assert_item_labels(&pane_a, ["B", "A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_beyond_unpinned_tab_in_same_pane_becomes_unpinned(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B to pane A and pin A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        add_labeled_item(&pane_a, "B", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B*"], cx);

        // Drag pinned A right of B in the same pane
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
        });

        // A becomes unpinned
        assert_item_labels(&pane_a, ["B", "A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_in_front_of_pinned_tab_in_same_pane_becomes_pinned(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B to pane A and pin A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B*"], cx);

        // Drag pinned B left of A in the same pane
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_b.boxed_clone(),
                ix: 1,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // A becomes unpinned
        assert_item_labels(&pane_a, ["B*!", "A!"], cx);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_to_the_pinned_region_stays_pinned(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C to pane A and pin A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        add_labeled_item(&pane_a, "B", false, cx);
        let item_c = add_labeled_item(&pane_a, "C", false, cx);
        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B", "C*"], cx);

        // Drag pinned C left of B in the same pane
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_c.boxed_clone(),
                ix: 2,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // A stays pinned, B and C remain unpinned
        assert_item_labels(&pane_a, ["A!", "C*", "B"], cx);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_into_existing_panes_pinned_region(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add unpinned item A to pane A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        assert_item_labels(&pane_a, ["A*"], cx);

        // Create pane B with pinned item B
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        let item_b = add_labeled_item(&pane_b, "B", false, cx);
        pane_b.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_b, ["B*!"], cx);

        // Move A from pane A to pane B's pinned region
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // A should become pinned since it was dropped in the pinned region
        assert_item_labels(&pane_a, [], cx);
        assert_item_labels(&pane_b, ["A*!", "B!"], cx);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_into_existing_panes_unpinned_region(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add unpinned item A to pane A
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        assert_item_labels(&pane_a, ["A*"], cx);

        // Create pane B with one pinned item B
        let pane_b = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
        });
        let item_b = add_labeled_item(&pane_b, "B", false, cx);
        pane_b.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_b, ["B*!"], cx);

        // Move A from pane A to pane B's unpinned region
        pane_b.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, true, window, cx);
        });

        // A should remain unpinned since it was dropped outside the pinned region
        assert_item_labels(&pane_a, [], cx);
        assert_item_labels(&pane_b, ["B!", "A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_throughout_entire_range_of_pinned_tabs_both_directions(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C and pin all
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        let item_b = add_labeled_item(&pane_a, "B", false, cx);
        let item_c = add_labeled_item(&pane_a, "C", false, cx);
        assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

        pane_a.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);

            let ix = pane.index_for_item_id(item_c.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane_a, ["A!", "B!", "C*!"], cx);

        // Move A to right of B
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // A should be after B and all are pinned
        assert_item_labels(&pane_a, ["B!", "A*!", "C!"], cx);

        // Move A to right of C
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 1,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
        });

        // A should be after C and all are pinned
        assert_item_labels(&pane_a, ["B!", "C!", "A*!"], cx);

        // Move A to left of C
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 2,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 1, false, window, cx);
        });

        // A should be before C and all are pinned
        assert_item_labels(&pane_a, ["B!", "A*!", "C!"], cx);

        // Move A to left of B
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 1,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // A should be before B and all are pinned
        assert_item_labels(&pane_a, ["A*!", "B!", "C!"], cx);
    }

    #[gpui::test]
    async fn test_drag_first_tab_to_last_position(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C
        let item_a = add_labeled_item(&pane_a, "A", false, cx);
        add_labeled_item(&pane_a, "B", false, cx);
        add_labeled_item(&pane_a, "C", false, cx);
        assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

        // Move A to the end
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_a.boxed_clone(),
                ix: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 2, false, window, cx);
        });

        // A should be at the end
        assert_item_labels(&pane_a, ["B", "C", "A*"], cx);
    }

    #[gpui::test]
    async fn test_drag_last_tab_to_first_position(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane_a = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add A, B, C
        add_labeled_item(&pane_a, "A", false, cx);
        add_labeled_item(&pane_a, "B", false, cx);
        let item_c = add_labeled_item(&pane_a, "C", false, cx);
        assert_item_labels(&pane_a, ["A", "B", "C*"], cx);

        // Move C to the beginning
        pane_a.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: pane_a.clone(),
                item: item_c.boxed_clone(),
                ix: 2,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, false, window, cx);
        });

        // C should be at the beginning
        assert_item_labels(&pane_a, ["C*", "A", "B"], cx);
    }

    #[gpui::test]
    async fn test_drag_tab_to_middle_tab_with_mouse_events(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
        cx.run_until_parked();

        let tab_a_bounds = cx
            .debug_bounds("TAB-0")
            .expect("Tab A (index 0) should have debug bounds");
        let tab_c_bounds = cx
            .debug_bounds("TAB-2")
            .expect("Tab C (index 2) should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: tab_a_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: tab_c_bounds.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
        cx.simulate_event(MouseUpEvent {
            position: tab_c_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_item_labels(&pane, ["B", "C", "A*", "D"], cx);
    }

    #[gpui::test]
    async fn test_drag_pinned_tab_when_show_pinned_tabs_in_separate_row_enabled(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        set_pinned_tabs_separate_row(cx, true);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        let item_b = add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        let item_d = add_labeled_item(&pane, "D", false, cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(
                pane.index_for_item_id(item_a.item_id()).unwrap(),
                window,
                cx,
            );
            pane.pin_tab_at(
                pane.index_for_item_id(item_b.item_id()).unwrap(),
                window,
                cx,
            );
            pane.pin_tab_at(
                pane.index_for_item_id(item_c.item_id()).unwrap(),
                window,
                cx,
            );
            pane.pin_tab_at(
                pane.index_for_item_id(item_d.item_id()).unwrap(),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A!", "B!", "C!", "D*!"], cx);
        cx.run_until_parked();

        let tab_a_bounds = cx
            .debug_bounds("TAB-0")
            .expect("Tab A (index 0) should have debug bounds");
        let tab_c_bounds = cx
            .debug_bounds("TAB-2")
            .expect("Tab C (index 2) should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: tab_a_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: tab_c_bounds.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
        cx.simulate_event(MouseUpEvent {
            position: tab_c_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_item_labels(&pane, ["B!", "C!", "A*!", "D!"], cx);
    }

    #[gpui::test]
    async fn test_drag_unpinned_tab_when_show_pinned_tabs_in_separate_row_enabled(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        set_pinned_tabs_separate_row(cx, true);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
        cx.run_until_parked();

        let tab_a_bounds = cx
            .debug_bounds("TAB-0")
            .expect("Tab A (index 0) should have debug bounds");
        let tab_c_bounds = cx
            .debug_bounds("TAB-2")
            .expect("Tab C (index 2) should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: tab_a_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: tab_c_bounds.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
        cx.simulate_event(MouseUpEvent {
            position: tab_c_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_item_labels(&pane, ["B", "C", "A*", "D"], cx);
    }

    #[gpui::test]
    async fn test_drag_mixed_tabs_when_show_pinned_tabs_in_separate_row_enabled(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        set_pinned_tabs_separate_row(cx, true);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        let item_b = add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        add_labeled_item(&pane, "F", false, cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(
                pane.index_for_item_id(item_a.item_id()).unwrap(),
                window,
                cx,
            );
            pane.pin_tab_at(
                pane.index_for_item_id(item_b.item_id()).unwrap(),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A!", "B!", "C", "D", "E", "F*"], cx);
        cx.run_until_parked();

        let tab_c_bounds = cx
            .debug_bounds("TAB-2")
            .expect("Tab C (index 2) should have debug bounds");
        let tab_e_bounds = cx
            .debug_bounds("TAB-4")
            .expect("Tab E (index 4) should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: tab_c_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: tab_e_bounds.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
        cx.simulate_event(MouseUpEvent {
            position: tab_e_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_item_labels(&pane, ["A!", "B!", "D", "E", "C*", "F"], cx);
    }

    #[gpui::test]
    async fn test_middle_click_pinned_tab_does_not_close(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(
                pane.index_for_item_id(item_a.item_id()).unwrap(),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A!", "B*"], cx);
        cx.run_until_parked();

        let tab_a_bounds = cx
            .debug_bounds("TAB-0")
            .expect("Tab A (index 1) should have debug bounds");
        let tab_b_bounds = cx
            .debug_bounds("TAB-1")
            .expect("Tab B (index 2) should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: tab_a_bounds.center(),
            button: MouseButton::Middle,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });

        cx.run_until_parked();

        cx.simulate_event(MouseUpEvent {
            position: tab_a_bounds.center(),
            button: MouseButton::Middle,
            modifiers: Modifiers::default(),
            click_count: 1,
        });

        cx.run_until_parked();

        cx.simulate_event(MouseDownEvent {
            position: tab_b_bounds.center(),
            button: MouseButton::Middle,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });

        cx.run_until_parked();

        cx.simulate_event(MouseUpEvent {
            position: tab_b_bounds.center(),
            button: MouseButton::Middle,
            modifiers: Modifiers::default(),
            click_count: 1,
        });

        cx.run_until_parked();

        assert_item_labels(&pane, ["A*!"], cx);
    }

    #[gpui::test]
    async fn test_double_click_pinned_tab_bar_empty_space_creates_new_tab(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // The real NewFile handler lives in editor::init, which isn't initialized
        // in workspace tests. Register a global action handler that sets a flag so
        // we can verify the action is dispatched without depending on the editor crate.
        // TODO: If editor::init is ever available in workspace tests, remove this
        // flag and assert the resulting tab bar state directly instead.
        let new_file_dispatched = Rc::new(Cell::new(false));
        cx.update(|_, cx| {
            let new_file_dispatched = new_file_dispatched.clone();
            cx.on_action(move |_: &NewFile, _cx| {
                new_file_dispatched.set(true);
            });
        });

        set_pinned_tabs_separate_row(cx, true);

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane
                .index_for_item_id(item_a.item_id())
                .expect("item A should exist");
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*"], cx);
        cx.run_until_parked();

        let pinned_drop_target_bounds = cx
            .debug_bounds("pinned_tabs_border")
            .expect("pinned_tabs_border should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: pinned_drop_target_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 2,
            first_mouse: false,
        });

        cx.run_until_parked();

        cx.simulate_event(MouseUpEvent {
            position: pinned_drop_target_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 2,
        });

        cx.run_until_parked();

        // TODO: If editor::init is ever available in workspace tests, replace this
        // with an assert_item_labels check that verifies a new tab is actually created.
        assert!(
            new_file_dispatched.get(),
            "Double-clicking pinned tab bar empty space should dispatch the new file action"
        );
    }

    #[gpui::test]
    async fn test_add_item_with_new_item(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // 1. Add with a destination index
        //   a. Add before the active item
        set_labeled_items(&pane, ["A", "B*", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
                false,
                false,
                Some(0),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["D*", "A", "B", "C"], cx);

        //   b. Add after the active item
        set_labeled_items(&pane, ["A", "B*", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
                false,
                false,
                Some(2),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A", "B", "D*", "C"], cx);

        //   c. Add at the end of the item list (including off the length)
        set_labeled_items(&pane, ["A", "B*", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
                false,
                false,
                Some(5),
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        // 2. Add without a destination index
        //   a. Add with active item at the start of the item list
        set_labeled_items(&pane, ["A*", "B", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        set_labeled_items(&pane, ["A", "D*", "B", "C"], cx);

        //   b. Add with active item at the end of the item list
        set_labeled_items(&pane, ["A", "B", "C*"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| TestItem::new(cx).with_label("D"))),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);
    }

    #[gpui::test]
    async fn test_add_item_with_existing_item(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // 1. Add with a destination index
        //   1a. Add before the active item
        let [_, _, _, d] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(d, false, false, Some(0), window, cx);
        });
        assert_item_labels(&pane, ["D*", "A", "B", "C"], cx);

        //   1b. Add after the active item
        let [_, _, _, d] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(d, false, false, Some(2), window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "D*", "C"], cx);

        //   1c. Add at the end of the item list (including off the length)
        let [a, _, _, _] = set_labeled_items(&pane, ["A", "B*", "C", "D"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(a, false, false, Some(5), window, cx);
        });
        assert_item_labels(&pane, ["B", "C", "D", "A*"], cx);

        //   1d. Add same item to active index
        let [_, b, _] = set_labeled_items(&pane, ["A", "B*", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(b, false, false, Some(1), window, cx);
        });
        assert_item_labels(&pane, ["A", "B*", "C"], cx);

        //   1e. Add item to index after same item in last position
        let [_, _, c] = set_labeled_items(&pane, ["A", "B*", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(c, false, false, Some(2), window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        // 2. Add without a destination index
        //   2a. Add with active item at the start of the item list
        let [_, _, _, d] = set_labeled_items(&pane, ["A*", "B", "C", "D"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(d, false, false, None, window, cx);
        });
        assert_item_labels(&pane, ["A", "D*", "B", "C"], cx);

        //   2b. Add with active item at the end of the item list
        let [a, _, _, _] = set_labeled_items(&pane, ["A", "B", "C", "D*"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(a, false, false, None, window, cx);
        });
        assert_item_labels(&pane, ["B", "C", "D", "A*"], cx);

        //   2c. Add active item to active item at end of list
        let [_, _, c] = set_labeled_items(&pane, ["A", "B", "C*"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(c, false, false, None, window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        //   2d. Add active item to active item at start of list
        let [a, _, _] = set_labeled_items(&pane, ["A*", "B", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(a, false, false, None, window, cx);
        });
        assert_item_labels(&pane, ["A*", "B", "C"], cx);
    }

    #[gpui::test]
    async fn test_add_item_with_same_project_entries(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // singleton view
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_buffer_kind(ItemBufferKind::Singleton)
                        .with_label("buffer 1")
                        .with_project_items(&[TestProjectItem::new(1, "one.txt", cx)])
                })),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["buffer 1*"], cx);

        // new singleton view with the same project entry
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_buffer_kind(ItemBufferKind::Singleton)
                        .with_label("buffer 1")
                        .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
                })),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["buffer 1*"], cx);

        // new singleton view with different project entry
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_buffer_kind(ItemBufferKind::Singleton)
                        .with_label("buffer 2")
                        .with_project_items(&[TestProjectItem::new(2, "2.txt", cx)])
                })),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["buffer 1", "buffer 2*"], cx);

        // new multibuffer view with the same project entry
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_buffer_kind(ItemBufferKind::Multibuffer)
                        .with_label("multibuffer 1")
                        .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
                })),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(&pane, ["buffer 1", "buffer 2", "multibuffer 1*"], cx);

        // another multibuffer view with the same project entry
        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(
                Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_buffer_kind(ItemBufferKind::Multibuffer)
                        .with_label("multibuffer 1b")
                        .with_project_items(&[TestProjectItem::new(1, "1.txt", cx)])
                })),
                false,
                false,
                None,
                window,
                cx,
            );
        });
        assert_item_labels(
            &pane,
            ["buffer 1", "buffer 2", "multibuffer 1", "multibuffer 1b*"],
            cx,
        );
    }

    #[gpui::test]
    async fn test_remove_item_ordering_history(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(1, false, false, window, cx)
        });
        add_labeled_item(&pane, "1", false, cx);
        assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B*", "C", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(3, false, false, window, cx)
        });
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B*", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A*"], cx);
    }

    #[gpui::test]
    async fn test_remove_item_ordering_neighbour(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update_global::<SettingsStore, ()>(|s, cx| {
            s.update_user_settings(cx, |s| {
                s.tabs.get_or_insert_default().activate_on_close = Some(ActivateOnClose::Neighbour);
            });
        });
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(1, false, false, window, cx)
        });
        add_labeled_item(&pane, "1", false, cx);
        assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B", "C*", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(3, false, false, window, cx)
        });
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A*"], cx);
    }

    #[gpui::test]
    async fn test_remove_item_ordering_left_neighbour(cx: &mut TestAppContext) {
        init_test(cx);
        cx.update_global::<SettingsStore, ()>(|s, cx| {
            s.update_user_settings(cx, |s| {
                s.tabs.get_or_insert_default().activate_on_close =
                    Some(ActivateOnClose::LeftNeighbour);
            });
        });
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(1, false, false, window, cx)
        });
        add_labeled_item(&pane, "1", false, cx);
        assert_item_labels(&pane, ["A", "B", "1*", "C", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B*", "C", "D"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(3, false, false, window, cx)
        });
        assert_item_labels(&pane, ["A", "B", "C", "D*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(0, false, false, window, cx)
        });
        assert_item_labels(&pane, ["A*", "B", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["B*", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["C*"], cx);
    }

    #[gpui::test]
    async fn test_close_inactive_items(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A*!"], cx);

        let item_b = add_labeled_item(&pane, "B", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_b.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
        });
        assert_item_labels(&pane, ["A!", "B*!"], cx);

        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C*"], cx);

        add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A!", "B!", "C", "D", "E*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_other_items(
                &CloseOtherItems {
                    save_intent: None,
                    close_pinned: false,
                },
                None,
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A!", "B!", "E*"], cx);
    }

    #[gpui::test]
    async fn test_running_close_inactive_items_via_an_inactive_item(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        assert_item_labels(&pane, ["A*"], cx);

        let item_b = add_labeled_item(&pane, "B", false, cx);
        assert_item_labels(&pane, ["A", "B*"], cx);

        add_labeled_item(&pane, "C", false, cx);
        add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A", "B", "C", "D", "E*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_other_items(
                &CloseOtherItems {
                    save_intent: None,
                    close_pinned: false,
                },
                Some(item_b.item_id()),
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["B*"], cx);
    }

    #[gpui::test]
    async fn test_close_other_items_unpreviews_active_item(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        let item_c = add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update(cx, |pane, cx| {
            pane.set_preview_item_id(Some(item_c.item_id()), cx);
        });
        assert!(pane.read_with(cx, |pane, _| pane.preview_item_id()
            == Some(item_c.item_id())));

        pane.update_in(cx, |pane, window, cx| {
            pane.close_other_items(
                &CloseOtherItems {
                    save_intent: None,
                    close_pinned: false,
                },
                Some(item_c.item_id()),
                window,
                cx,
            )
        })
        .await
        .unwrap();

        assert!(pane.read_with(cx, |pane, _| pane.preview_item_id().is_none()));
        assert_item_labels(&pane, ["C*"], cx);
    }

    #[gpui::test]
    async fn test_close_clean_items(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", true, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", true, cx);
        add_labeled_item(&pane, "D", false, cx);
        add_labeled_item(&pane, "E", false, cx);
        assert_item_labels(&pane, ["A^", "B", "C^", "D", "E*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_clean_items(
                &CloseCleanItems {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A^", "C*^"], cx);
    }

    #[gpui::test]
    async fn test_close_items_to_the_left(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_labeled_items(&pane, ["A", "B", "C*", "D", "E"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_items_to_the_left_by_id(
                None,
                &CloseItemsToTheLeft {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["C*", "D", "E"], cx);
    }

    #[gpui::test]
    async fn test_close_items_to_the_right(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        set_labeled_items(&pane, ["A", "B", "C*", "D", "E"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_items_to_the_right_by_id(
                None,
                &CloseItemsToTheRight {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A", "B", "C*"], cx);
    }

    #[gpui::test]
    async fn test_close_all_items(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A*!"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "A", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "A.txt", cx))
        });
        add_labeled_item(&pane, "B", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "B.txt", cx))
        });
        add_labeled_item(&pane, "C", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(3, "C.txt", cx))
        });
        assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Save all");
        save.await.unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "A", true, cx);
        add_labeled_item(&pane, "B", true, cx);
        add_labeled_item(&pane, "C", true, cx);
        assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);
        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        save.await.unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "A", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "A.txt", cx))
        });
        add_labeled_item(&pane, "B", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "B.txt", cx))
        });
        add_labeled_item(&pane, "C", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(3, "C.txt", cx))
        });
        assert_item_labels(&pane, ["A^", "B^", "C*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "Clean1", false, cx);
        add_labeled_item(&pane, "Dirty", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "Dirty.txt", cx))
        });
        add_labeled_item(&pane, "Clean2", false, cx);
        assert_item_labels(&pane, ["Clean1", "Dirty^", "Clean2*"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Cancel");
        close_task.await.unwrap();
        assert_item_labels(&pane, ["Dirty*^"], cx);
    }

    #[gpui::test]
    async fn test_discard_all_reloads_from_disk(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", true, cx);
        item_a.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "A.txt", cx))
        });
        let item_b = add_labeled_item(&pane, "B", true, cx);
        item_b.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "B.txt", cx))
        });
        assert_item_labels(&pane, ["A^", "B*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        item_a.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "item A should have been reloaded");
            assert!(
                !item.is_dirty,
                "item A should no longer be dirty after reload"
            );
        });
        item_b.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "item B should have been reloaded");
            assert!(
                !item.is_dirty,
                "item B should no longer be dirty after reload"
            );
        });
    }

    #[gpui::test]
    async fn test_dont_save_single_file_reloads_from_disk(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item = add_labeled_item(&pane, "Dirty", true, cx);
        item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "Dirty.txt", cx))
        });
        assert_item_labels(&pane, ["Dirty*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_item_by_id(item.item_id(), SaveIntent::Close, window, cx)
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        item.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "item should have been reloaded");
            assert!(
                !item.is_dirty,
                "item should no longer be dirty after reload"
            );
        });
    }

    #[gpui::test]
    async fn test_format_runs_on_first_save_of_new_file(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item = add_labeled_item(&pane, "untitled", true, cx);
        item.update(cx, |item, cx| {
            item.project_items.push(TestProjectItem::new_untitled(cx));
        });
        assert_item_labels(&pane, ["untitled*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_item_by_id(item.item_id(), SaveIntent::Save, window, cx)
        });

        cx.executor().run_until_parked();
        cx.simulate_new_path_selection(|_| Some(Default::default()));
        close_task.await.unwrap();

        item.read_with(cx, |item, _| {
            assert_eq!(item.save_as_count, 1);
            assert_eq!(
                item.save_count, 1,
                "formatter should run after the file is given a path on first save"
            );
        });
    }

    #[gpui::test]
    async fn test_format_does_not_run_on_first_save_when_save_without_format(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item = add_labeled_item(&pane, "untitled", true, cx);
        item.update(cx, |item, cx| {
            item.project_items.push(TestProjectItem::new_untitled(cx));
        });
        assert_item_labels(&pane, ["untitled*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_item_by_id(item.item_id(), SaveIntent::SaveWithoutFormat, window, cx)
        });

        cx.executor().run_until_parked();
        cx.simulate_new_path_selection(|_| Some(Default::default()));
        close_task.await.unwrap();

        item.read_with(cx, |item, _| {
            assert_eq!(item.save_as_count, 1);
            assert_eq!(
                item.save_count, 0,
                "formatter should not run when SaveWithoutFormat is used"
            );
        });
    }

    #[gpui::test]
    async fn test_discard_does_not_reload_multibuffer(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let singleton_item = pane.update_in(cx, |pane, window, cx| {
            let item = Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_label("Singleton")
                    .with_dirty(true)
                    .with_buffer_kind(ItemBufferKind::Singleton)
            }));
            pane.add_item(item.clone(), false, false, None, window, cx);
            item
        });
        singleton_item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "Singleton.txt", cx))
        });

        let multi_item = pane.update_in(cx, |pane, window, cx| {
            let item = Box::new(cx.new(|cx| {
                TestItem::new(cx)
                    .with_label("Multi")
                    .with_dirty(true)
                    .with_buffer_kind(ItemBufferKind::Multibuffer)
            }));
            pane.add_item(item.clone(), false, false, None, window, cx);
            item
        });
        multi_item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "Multi.txt", cx))
        });

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        singleton_item.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "singleton should have been reloaded");
            assert!(
                !item.is_dirty,
                "singleton should no longer be dirty after reload"
            );
        });
        multi_item.read_with(cx, |item, _| {
            assert_eq!(
                item.reload_count, 0,
                "multibuffer should not have been reloaded"
            );
        });
    }

    #[gpui::test]
    async fn test_close_multibuffer_items(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let add_labeled_item = |pane: &Entity<Pane>,
                                label,
                                is_dirty,
                                kind: ItemBufferKind,
                                cx: &mut VisualTestContext| {
            pane.update_in(cx, |pane, window, cx| {
                let labeled_item = Box::new(cx.new(|cx| {
                    TestItem::new(cx)
                        .with_label(label)
                        .with_dirty(is_dirty)
                        .with_buffer_kind(kind)
                }));
                pane.add_item(labeled_item.clone(), false, false, None, window, cx);
                labeled_item
            })
        };

        let item_a = add_labeled_item(&pane, "A", false, ItemBufferKind::Multibuffer, cx);
        add_labeled_item(&pane, "B", false, ItemBufferKind::Multibuffer, cx);
        add_labeled_item(&pane, "C", false, ItemBufferKind::Singleton, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
            pane.close_multibuffer_items(
                &CloseMultibufferItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, ["A!", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.unpin_tab_at(ix, window, cx);
            pane.close_multibuffer_items(
                &CloseMultibufferItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        assert_item_labels(&pane, ["C*"], cx);

        add_labeled_item(&pane, "A", true, ItemBufferKind::Singleton, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "A.txt", cx))
        });
        add_labeled_item(&pane, "B", true, ItemBufferKind::Multibuffer, cx).update(
            cx,
            |item, cx| {
                item.project_items
                    .push(TestProjectItem::new_dirty(2, "B.txt", cx))
            },
        );
        add_labeled_item(&pane, "D", true, ItemBufferKind::Multibuffer, cx).update(
            cx,
            |item, cx| {
                item.project_items
                    .push(TestProjectItem::new_dirty(3, "D.txt", cx))
            },
        );
        assert_item_labels(&pane, ["C", "A^", "B^", "D*^"], cx);

        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_multibuffer_items(
                &CloseMultibufferItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Save all");
        save.await.unwrap();
        assert_item_labels(&pane, ["C", "A*^"], cx);

        add_labeled_item(&pane, "B", true, ItemBufferKind::Multibuffer, cx).update(
            cx,
            |item, cx| {
                item.project_items
                    .push(TestProjectItem::new_dirty(2, "B.txt", cx))
            },
        );
        add_labeled_item(&pane, "D", true, ItemBufferKind::Multibuffer, cx).update(
            cx,
            |item, cx| {
                item.project_items
                    .push(TestProjectItem::new_dirty(3, "D.txt", cx))
            },
        );
        assert_item_labels(&pane, ["C", "A^", "B^", "D*^"], cx);
        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_multibuffer_items(
                &CloseMultibufferItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        save.await.unwrap();
        assert_item_labels(&pane, ["C", "A*^"], cx);
    }

    #[gpui::test]
    async fn test_close_with_save_intent(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let a = cx.update(|_, cx| TestProjectItem::new_dirty(1, "A.txt", cx));
        let b = cx.update(|_, cx| TestProjectItem::new_dirty(1, "B.txt", cx));
        let c = cx.update(|_, cx| TestProjectItem::new_dirty(1, "C.txt", cx));

        add_labeled_item(&pane, "AB", true, cx).update(cx, |item, _| {
            item.project_items.push(a.clone());
            item.project_items.push(b.clone());
        });
        add_labeled_item(&pane, "C", true, cx)
            .update(cx, |item, _| item.project_items.push(c.clone()));
        assert_item_labels(&pane, ["AB^", "C*^"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: Some(SaveIntent::Save),
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        assert_item_labels(&pane, [], cx);
        cx.update(|_, cx| {
            assert!(!a.read(cx).is_dirty);
            assert!(!b.read(cx).is_dirty);
            assert!(!c.read(cx).is_dirty);
        });
    }

    #[gpui::test]
    async fn test_new_tab_scrolls_into_view_completely(cx: &mut TestAppContext) {
        // Arrange
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        cx.simulate_resize(size(px(300.), px(300.)));

        add_labeled_item(&pane, "untitled", false, cx);
        add_labeled_item(&pane, "untitled", false, cx);
        add_labeled_item(&pane, "untitled", false, cx);
        add_labeled_item(&pane, "untitled", false, cx);
        // Act: this should trigger a scroll
        add_labeled_item(&pane, "untitled", false, cx);
        // Assert
        let tab_bar_scroll_handle =
            pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
        assert_eq!(tab_bar_scroll_handle.children_count(), 6);
        let tab_bounds = cx.debug_bounds("TAB-4").unwrap();
        let new_tab_button_bounds = cx.debug_bounds("ICON-Plus").unwrap();
        let scroll_bounds = tab_bar_scroll_handle.bounds();
        let scroll_offset = tab_bar_scroll_handle.offset();
        assert!(scroll_offset.x < px(0.));
        assert!(scroll_offset.x >= -tab_bar_scroll_handle.max_offset().x);
        assert!(tab_bounds.left() >= scroll_bounds.left());
        assert!(tab_bounds.right() <= scroll_bounds.right());
        assert!(
            !tab_bounds.intersects(&new_tab_button_bounds),
            "Tab should not overlap with the new tab button, if this is failing check if there's been a redesign!"
        );
    }

    #[gpui::test]
    async fn test_pinned_tabs_scroll_to_item_uses_correct_index(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        cx.simulate_resize(size(px(400.), px(300.)));

        for label in ["A", "B", "C"] {
            add_labeled_item(&pane, label, false, cx);
        }

        pane.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(0, window, cx);
            pane.pin_tab_at(1, window, cx);
            pane.pin_tab_at(2, window, cx);
        });

        for label in ["D", "E", "F", "G", "H", "I", "J", "K"] {
            add_labeled_item(&pane, label, false, cx);
        }

        assert_item_labels(
            &pane,
            ["A!", "B!", "C!", "D", "E", "F", "G", "H", "I", "J", "K*"],
            cx,
        );

        cx.run_until_parked();

        // Verify overflow exists (precondition for scroll test)
        let scroll_handle =
            pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
        assert!(
            scroll_handle.max_offset().x > px(0.),
            "Test requires tab overflow to verify scrolling. Increase tab count or reduce window width."
        );

        // Activate a different tab first, then activate K
        // This ensures we're not just re-activating an already-active tab
        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(3, true, true, window, cx);
        });
        cx.run_until_parked();

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(10, true, true, window, cx);
        });
        cx.run_until_parked();

        let scroll_handle =
            pane.update_in(cx, |pane, _window, _cx| pane.tab_bar_scroll_handle.clone());
        let k_tab_bounds = cx.debug_bounds("TAB-10").unwrap();
        let scroll_bounds = scroll_handle.bounds();

        assert!(
            k_tab_bounds.left() >= scroll_bounds.left(),
            "Active tab K should be scrolled into view"
        );
    }

    #[gpui::test]
    async fn test_close_all_items_including_pinned(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());

        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let item_a = add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let ix = pane.index_for_item_id(item_a.item_id()).unwrap();
            pane.pin_tab_at(ix, window, cx);
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: true,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, [], cx);
    }

    #[gpui::test]
    async fn test_close_pinned_tab_with_non_pinned_in_same_pane(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        // Non-pinned tabs in same pane
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(0, window, cx);
        });
        set_labeled_items(&pane, ["A*", "B", "C"], cx);
        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
            .unwrap();
        });
        // Non-pinned tab should be active
        assert_item_labels(&pane, ["A!", "B*", "C"], cx);
    }

    #[gpui::test]
    async fn test_close_pinned_tab_with_non_pinned_in_different_pane(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        // No non-pinned tabs in same pane, non-pinned tabs in another pane
        let pane1 = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        let pane2 = workspace.update_in(cx, |workspace, window, cx| {
            workspace.split_pane(pane1.clone(), SplitDirection::Right, window, cx)
        });
        add_labeled_item(&pane1, "A", false, cx);
        pane1.update_in(cx, |pane, window, cx| {
            pane.pin_tab_at(0, window, cx);
        });
        set_labeled_items(&pane1, ["A*"], cx);
        add_labeled_item(&pane2, "B", false, cx);
        set_labeled_items(&pane2, ["B"], cx);
        pane1.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
            .unwrap();
        });
        //  Non-pinned tab of other pane should be active
        assert_item_labels(&pane2, ["B*"], cx);
    }

    #[gpui::test]
    async fn ensure_item_closing_actions_do_not_panic_when_no_items_exist(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        assert_item_labels(&pane, [], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        pane.update_in(cx, |pane, window, cx| {
            pane.close_other_items(
                &CloseOtherItems {
                    save_intent: None,
                    close_pinned: false,
                },
                None,
                window,
                cx,
            )
        })
        .await
        .unwrap();

        pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &CloseAllItems {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        pane.update_in(cx, |pane, window, cx| {
            pane.close_clean_items(
                &CloseCleanItems {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        pane.update_in(cx, |pane, window, cx| {
            pane.close_items_to_the_right_by_id(
                None,
                &CloseItemsToTheRight {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        pane.update_in(cx, |pane, window, cx| {
            pane.close_items_to_the_left_by_id(
                None,
                &CloseItemsToTheLeft {
                    close_pinned: false,
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();
    }

    #[gpui::test]
    async fn test_item_swapping_actions(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        assert_item_labels(&pane, [], cx);

        // Test that these actions do not panic
        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_right(&Default::default(), window, cx);
        });

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_left(&Default::default(), window, cx);
        });

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_right(&Default::default(), window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_left(&Default::default(), window, cx);
        });
        assert_item_labels(&pane, ["A", "C*", "B"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_left(&Default::default(), window, cx);
        });
        assert_item_labels(&pane, ["C*", "A", "B"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_left(&Default::default(), window, cx);
        });
        assert_item_labels(&pane, ["C*", "A", "B"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.swap_item_right(&Default::default(), window, cx);
        });
        assert_item_labels(&pane, ["A", "C*", "B"], cx);
    }

    #[gpui::test]
    async fn test_split_empty(cx: &mut TestAppContext) {
        for split_direction in SplitDirection::all() {
            test_single_pane_split(["A"], split_direction, SplitMode::EmptyPane, cx).await;
        }
    }

    #[gpui::test]
    async fn test_split_clone(cx: &mut TestAppContext) {
        for split_direction in SplitDirection::all() {
            test_single_pane_split(["A"], split_direction, SplitMode::ClonePane, cx).await;
        }
    }

    #[gpui::test]
    async fn test_split_move_right_on_single_pane(cx: &mut TestAppContext) {
        test_single_pane_split(["A"], SplitDirection::Right, SplitMode::MovePane, cx).await;
    }

    #[gpui::test]
    async fn test_split_move(cx: &mut TestAppContext) {
        for split_direction in SplitDirection::all() {
            test_single_pane_split(["A", "B"], split_direction, SplitMode::MovePane, cx).await;
        }
    }

    #[gpui::test]
    async fn test_reopening_closed_item_after_unpreview(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update_global::<SettingsStore, ()>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.preview_tabs.get_or_insert_default().enabled = Some(true);
            });
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        // Add an item as preview
        let item = pane.update_in(cx, |pane, window, cx| {
            let item = Box::new(cx.new(|cx| TestItem::new(cx).with_label("A")));
            pane.add_item(item.clone(), true, true, None, window, cx);
            pane.set_preview_item_id(Some(item.item_id()), cx);
            item
        });

        // Verify item is preview
        pane.read_with(cx, |pane, _| {
            assert_eq!(pane.preview_item_id(), Some(item.item_id()));
        });

        // Unpreview the item
        pane.update_in(cx, |pane, _window, _cx| {
            pane.unpreview_item_if_preview(item.item_id());
        });

        // Verify item is no longer preview
        pane.read_with(cx, |pane, _| {
            assert_eq!(pane.preview_item_id(), None);
        });

        // Close the item
        pane.update_in(cx, |pane, window, cx| {
            pane.close_item_by_id(item.item_id(), SaveIntent::Skip, window, cx)
                .detach_and_log_err(cx);
        });

        cx.run_until_parked();

        // The item should be in the closed_stack and reopenable
        let has_closed_items = pane.read_with(cx, |pane, _| {
            !pane.nav_history.0.lock().closed_stack.is_empty()
        });
        assert!(
            has_closed_items,
            "closed item should be in closed_stack and reopenable"
        );
    }

    #[gpui::test]
    async fn test_activate_item_with_wrap_around(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));
        let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        add_labeled_item(&pane, "A", false, cx);
        add_labeled_item(&pane, "B", false, cx);
        add_labeled_item(&pane, "C", false, cx);
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_next_item(&ActivateNextItem { wrap_around: false }, window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_next_item(&ActivateNextItem::default(), window, cx);
        });
        assert_item_labels(&pane, ["A*", "B", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_previous_item(&ActivatePreviousItem { wrap_around: false }, window, cx);
        });
        assert_item_labels(&pane, ["A*", "B", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_previous_item(&ActivatePreviousItem::default(), window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_previous_item(&ActivatePreviousItem { wrap_around: false }, window, cx);
        });
        assert_item_labels(&pane, ["A", "B*", "C"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_next_item(&ActivateNextItem { wrap_around: false }, window, cx);
        });
        assert_item_labels(&pane, ["A", "B", "C*"], cx);
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(LoadThemes::JustBase, cx);
        });
    }

    fn set_max_tabs(cx: &mut TestAppContext, value: Option<usize>) {
        cx.update_global(|store: &mut SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings.workspace.max_tabs = value.map(|v| NonZero::new(v).unwrap())
            });
        });
    }

    fn set_pinned_tabs_separate_row(cx: &mut TestAppContext, enabled: bool) {
        cx.update_global(|store: &mut SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .tab_bar
                    .get_or_insert_default()
                    .show_pinned_tabs_in_separate_row = Some(enabled);
            });
        });
    }

    fn add_labeled_item(
        pane: &Entity<Pane>,
        label: &str,
        is_dirty: bool,
        cx: &mut VisualTestContext,
    ) -> Box<Entity<TestItem>> {
        pane.update_in(cx, |pane, window, cx| {
            let labeled_item =
                Box::new(cx.new(|cx| TestItem::new(cx).with_label(label).with_dirty(is_dirty)));
            pane.add_item(labeled_item.clone(), false, false, None, window, cx);
            labeled_item
        })
    }

    fn set_labeled_items<const COUNT: usize>(
        pane: &Entity<Pane>,
        labels: [&str; COUNT],
        cx: &mut VisualTestContext,
    ) -> [Box<Entity<TestItem>>; COUNT] {
        pane.update_in(cx, |pane, window, cx| {
            pane.items.clear();
            let mut active_item_index = 0;

            let mut index = 0;
            let items = labels.map(|mut label| {
                if label.ends_with('*') {
                    label = label.trim_end_matches('*');
                    active_item_index = index;
                }

                let labeled_item = Box::new(cx.new(|cx| TestItem::new(cx).with_label(label)));
                pane.add_item(labeled_item.clone(), false, false, None, window, cx);
                index += 1;
                labeled_item
            });

            pane.activate_item(active_item_index, false, false, window, cx);

            items
        })
    }

    // Assert the item label, with the active item label suffixed with a '*'
    #[track_caller]
    fn assert_item_labels<const COUNT: usize>(
        pane: &Entity<Pane>,
        expected_states: [&str; COUNT],
        cx: &mut VisualTestContext,
    ) {
        let actual_states = pane.update(cx, |pane, cx| {
            pane.items
                .iter()
                .enumerate()
                .map(|(ix, item)| {
                    let mut state = item
                        .to_any_view()
                        .downcast::<TestItem>()
                        .unwrap()
                        .read(cx)
                        .label
                        .clone();
                    if ix == pane.active_item_index {
                        state.push('*');
                    }
                    if item.is_dirty(cx) {
                        state.push('^');
                    }
                    if pane.is_tab_pinned(ix) {
                        state.push('!');
                    }
                    state
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            actual_states, expected_states,
            "pane items do not match expectation"
        );
    }

    // Assert the item label, with the active item label expected active index
    #[track_caller]
    fn assert_item_labels_active_index(
        pane: &Entity<Pane>,
        expected_states: &[&str],
        expected_active_idx: usize,
        cx: &mut VisualTestContext,
    ) {
        let actual_states = pane.update(cx, |pane, cx| {
            pane.items
                .iter()
                .enumerate()
                .map(|(ix, item)| {
                    let mut state = item
                        .to_any_view()
                        .downcast::<TestItem>()
                        .unwrap()
                        .read(cx)
                        .label
                        .clone();
                    if ix == pane.active_item_index {
                        assert_eq!(ix, expected_active_idx);
                    }
                    if item.is_dirty(cx) {
                        state.push('^');
                    }
                    if pane.is_tab_pinned(ix) {
                        state.push('!');
                    }
                    state
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(
            actual_states, expected_states,
            "pane items do not match expectation"
        );
    }

    #[track_caller]
    fn assert_pane_ids_on_axis<const COUNT: usize>(
        workspace: &Entity<Workspace>,
        expected_ids: [&EntityId; COUNT],
        expected_axis: Axis,
        cx: &mut VisualTestContext,
    ) {
        workspace.read_with(cx, |workspace, _| match &workspace.center.root {
            Member::Axis(axis) => {
                assert_eq!(axis.axis, expected_axis);
                assert_eq!(axis.members.len(), expected_ids.len());
                assert!(
                    zip(expected_ids, &axis.members).all(|(e, a)| {
                        if let Member::Pane(p) = a {
                            p.entity_id() == *e
                        } else {
                            false
                        }
                    }),
                    "pane ids do not match expectation: {expected_ids:?} != {actual_ids:?}",
                    actual_ids = axis.members
                );
            }
            Member::Pane(_) => panic!("expected axis"),
        });
    }

    async fn test_single_pane_split<const COUNT: usize>(
        pane_labels: [&str; COUNT],
        direction: SplitDirection,
        operation: SplitMode,
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project, window, cx));

        let mut pane_before =
            workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());
        for label in pane_labels {
            add_labeled_item(&pane_before, label, false, cx);
        }
        pane_before.update_in(cx, |pane, window, cx| {
            pane.split(direction, operation, window, cx)
        });
        cx.executor().run_until_parked();
        let pane_after = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

        let num_labels = pane_labels.len();
        let last_as_active = format!("{}*", String::from(pane_labels[num_labels - 1]));

        // check labels for all split operations
        match operation {
            SplitMode::EmptyPane => {
                assert_item_labels_active_index(&pane_before, &pane_labels, num_labels - 1, cx);
                assert_item_labels(&pane_after, [], cx);
            }
            SplitMode::ClonePane => {
                assert_item_labels_active_index(&pane_before, &pane_labels, num_labels - 1, cx);
                assert_item_labels(&pane_after, [&last_as_active], cx);
            }
            SplitMode::MovePane => {
                let head = &pane_labels[..(num_labels - 1)];
                if num_labels == 1 {
                    // We special-case this behavior and actually execute an empty pane command
                    // followed by a refocus of the old pane for this case.
                    pane_before = workspace.read_with(cx, |workspace, _cx| {
                        workspace
                            .panes()
                            .into_iter()
                            .find(|pane| *pane != &pane_after)
                            .unwrap()
                            .clone()
                    });
                };

                assert_item_labels_active_index(
                    &pane_before,
                    &head,
                    head.len().saturating_sub(1),
                    cx,
                );
                assert_item_labels(&pane_after, [&last_as_active], cx);
                pane_after.update_in(cx, |pane, window, cx| {
                    window.focused(cx).is_some_and(|focus_handle| {
                        focus_handle == pane.active_item().unwrap().item_focus_handle(cx)
                    })
                });
            }
        }

        // expected axis depends on split direction
        let expected_axis = match direction {
            SplitDirection::Right | SplitDirection::Left => Axis::Horizontal,
            SplitDirection::Up | SplitDirection::Down => Axis::Vertical,
        };

        // expected ids depends on split direction
        let expected_ids = match direction {
            SplitDirection::Right | SplitDirection::Down => {
                [&pane_before.entity_id(), &pane_after.entity_id()]
            }
            SplitDirection::Left | SplitDirection::Up => {
                [&pane_after.entity_id(), &pane_before.entity_id()]
            }
        };

        // check pane axes for all operations
        match operation {
            SplitMode::EmptyPane | SplitMode::ClonePane => {
                assert_pane_ids_on_axis(&workspace, expected_ids, expected_axis, cx);
            }
            SplitMode::MovePane => {
                assert_pane_ids_on_axis(&workspace, expected_ids, expected_axis, cx);
            }
        }
    }

    mod property_test {
        use super::*;
        use proptest::prelude::*;
        use serde_json::json;
        use std::{collections::HashSet, sync::Arc};
        use util::{
            path,
            rel_path::{RelPath, rel_path},
        };

        struct TestFileItem {
            project_path: ProjectPath,
        }

        impl project::ProjectItem for TestFileItem {
            fn try_open(
                _project: &Entity<Project>,
                path: &ProjectPath,
                cx: &mut App,
            ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
                let project_path = path.clone();
                Some(cx.spawn(async move |cx| Ok(cx.new(|_| Self { project_path }))))
            }

            fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
                None
            }

            fn project_path(&self, _: &App) -> Option<ProjectPath> {
                Some(self.project_path.clone())
            }

            fn is_dirty(&self) -> bool {
                false
            }
        }

        struct TestItemView {
            focus_handle: FocusHandle,
            project_item: Entity<TestFileItem>,
            nav_history: Option<ItemNavHistory>,
        }

        impl EventEmitter<()> for TestItemView {}

        impl Focusable for TestItemView {
            fn focus_handle(&self, _cx: &App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }

        impl Render for TestItemView {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                gpui::Empty
            }
        }

        impl Item for TestItemView {
            type Event = ();

            fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
                "".into()
            }

            fn for_each_project_item(
                &self,
                cx: &App,
                f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
            ) {
                f(self.project_item.entity_id(), self.project_item.read(cx))
            }

            fn buffer_kind(&self, _: &App) -> ItemBufferKind {
                ItemBufferKind::Singleton
            }

            fn set_nav_history(
                &mut self,
                history: ItemNavHistory,
                _window: &mut Window,
                _: &mut Context<Self>,
            ) {
                self.nav_history = Some(history);
            }

            fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
                if let Some(nav_history) = self.nav_history.as_mut() {
                    nav_history.push::<()>(None, None, cx);
                }
            }
        }

        impl crate::ProjectItem for TestItemView {
            type Item = TestFileItem;

            fn for_project_item(
                _project: Entity<Project>,
                _pane: Option<&Pane>,
                item: Entity<Self::Item>,
                _: &mut Window,
                cx: &mut Context<Self>,
            ) -> Self
            where
                Self: Sized,
            {
                Self {
                    focus_handle: cx.focus_handle(),
                    project_item: item,
                    nav_history: None,
                }
            }
        }

        fn arbitrary_path() -> impl Strategy<Value = Arc<RelPath>> {
            prop_oneof![
                Just(rel_path("1.txt").into()),
                Just(rel_path("2.js").into()),
                Just(rel_path("3.rs").into()),
            ]
        }

        #[derive(Debug, Clone, proptest_derive::Arbitrary)]
        enum Operation {
            Open {
                #[proptest(strategy = "arbitrary_path()")]
                path: Arc<RelPath>,
                allow_preview: bool,
            },
            GoBack,
            GoForward,
        }

        struct Oracle {
            /// The active item's path, if known.
            current: Option<Arc<RelPath>>,
            /// The path that the back button would navigate to, if known.
            previous: Option<Arc<RelPath>>,
            /// The path that the forward button would navigate to, if known.
            next: Option<Arc<RelPath>>,
        }

        impl Oracle {
            fn new() -> Self {
                Self {
                    current: None,
                    previous: None,
                    next: None,
                }
            }

            fn apply(&mut self, operation: Operation) {
                match operation {
                    Operation::Open { path, .. } => {
                        if self.current.as_ref() != Some(&path) {
                            self.previous = self.current.replace(path);
                            self.next = None;
                        }
                    }
                    Operation::GoBack => {
                        if let Some(previous) = self.previous.take() {
                            self.next = self.current.replace(previous);
                        } else {
                            // `previous` isn't set, so backward navigation may not have been
                            // possible, hence we don't know which item a following forward
                            // navigation will lead to
                            self.next = None;
                            self.current = None;
                        }
                    }
                    Operation::GoForward => {
                        if let Some(next) = self.next.take() {
                            self.previous = self.current.replace(next);
                        } else {
                            self.previous = None;
                            self.current = None;
                        }
                    }
                }
            }
        }

        struct PaneHarness {
            cx: VisualTestContext,
            workspace: Entity<Workspace>,
            pane: Entity<Pane>,
            worktree_id: WorktreeId,
        }

        impl PaneHarness {
            async fn new(cx: &mut TestAppContext) -> Self {
                init_test(cx);
                cx.update(|cx| crate::register_project_item::<TestItemView>(cx));

                let fs = FakeFs::new(cx.executor());
                fs.insert_tree(
                    path!("/root"),
                    json!({
                        "1.txt": "one",
                        "2.js": "two",
                        "3.rs": "three",
                    }),
                )
                .await;
                let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
                let worktree_id = project.update(cx, |project, cx| {
                    project.worktrees(cx).next().unwrap().read(cx).id()
                });
                let window = cx.add_window(|window, cx| Workspace::test_new(project, window, cx));
                let workspace = window.root(cx).unwrap();
                let cx = VisualTestContext::from_window(*window, cx);
                let pane = workspace.read_with(&cx, |workspace, _| workspace.active_pane().clone());

                Self {
                    cx,
                    workspace,
                    pane,
                    worktree_id,
                }
            }

            async fn apply(&mut self, operation: Operation) {
                match operation {
                    Operation::Open {
                        path,
                        allow_preview,
                    } => {
                        self.workspace
                            .update_in(&mut self.cx, |workspace, window, cx| {
                                workspace.open_path_preview_in_tabbed_pane(
                                    ProjectPath {
                                        worktree_id: self.worktree_id,
                                        path,
                                    },
                                    None,
                                    true,
                                    allow_preview,
                                    true,
                                    window,
                                    cx,
                                )
                            })
                            .await
                            .unwrap();
                    }
                    Operation::GoBack => {
                        self.workspace
                            .update_in(&mut self.cx, |workspace, window, cx| {
                                workspace.go_back(self.pane.downgrade(), window, cx)
                            })
                            .await
                            .unwrap();
                    }
                    Operation::GoForward => {
                        self.workspace
                            .update_in(&mut self.cx, |workspace, window, cx| {
                                workspace.go_forward(self.pane.downgrade(), window, cx)
                            })
                            .await
                            .unwrap();
                    }
                }
            }

            fn check_invariants(&self, expected_path: &Option<Arc<RelPath>>) {
                self.pane.read_with(&self.cx, |pane, cx| {
                    let open_paths = pane
                        .items()
                        .map(|item| item.project_path(cx).unwrap())
                        .collect::<Vec<_>>();
                    let active_path = pane
                        .active_item()
                        .map(|item| item.project_path(cx).unwrap());
                    let preview_path = pane
                        .preview_item()
                        .map(|item| item.project_path(cx).unwrap());

                    let unique_paths = open_paths.iter().collect::<HashSet<_>>();
                    assert_eq!(
                        unique_paths.len(),
                        open_paths.len(),
                        "pane should not contain duplicate open paths"
                    );

                    assert_eq!(
                        active_path.is_none(),
                        open_paths.is_empty(),
                        "pane should have an active item iff it has open paths"
                    );
                    assert!(
                        active_path
                            .as_ref()
                            .is_none_or(|path| open_paths.contains(path)),
                        "active path should be open"
                    );
                    assert!(
                        preview_path
                            .as_ref()
                            .is_none_or(|path| open_paths.contains(path)),
                        "preview path should be open"
                    );

                    if let Some(expected_active_path) = expected_path {
                        assert_eq!(
                            &active_path.as_ref().unwrap().path,
                            expected_active_path,
                            "active path should match the oracle"
                        );
                    }
                });
            }
        }

        #[gpui::property_test]
        async fn single_pane_navigation(
            #[strategy = proptest::collection::vec(any::<Operation>(), 1..32)] operations: Vec<
                Operation,
            >,
            cx: &mut TestAppContext,
        ) {
            let mut harness = PaneHarness::new(cx).await;
            let mut oracle = Oracle::new();

            for operation in operations {
                oracle.apply(operation.clone());
                harness.apply(operation).await;
                harness.check_invariants(&oracle.current);
            }
        }
    }
}
