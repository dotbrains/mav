#[path = "outline_panel/active_editor_state.rs"]
mod active_editor_state;
#[path = "outline_panel/buffer_outline_state.rs"]
mod buffer_outline_state;
#[path = "outline_panel/cached_entry_generation.rs"]
mod cached_entry_generation;
#[path = "outline_panel/cached_entry_helpers.rs"]
mod cached_entry_helpers;
#[path = "outline_panel/context_actions.rs"]
mod context_actions;
#[path = "outline_panel/entry_types.rs"]
mod entry_types;
#[path = "outline_panel/expand_collapse.rs"]
mod expand_collapse;
#[path = "outline_panel/fs_entries_update.rs"]
mod fs_entries_update;
#[path = "outline_panel/main_content_rendering.rs"]
mod main_content_rendering;
mod outline_panel_settings;
#[path = "outline_panel/panel_helpers.rs"]
mod panel_helpers;
#[path = "outline_panel/panel_runtime.rs"]
mod panel_runtime;
#[path = "outline_panel/rendering.rs"]
mod rendering;
#[path = "outline_panel/search_state.rs"]
mod search_state;
#[path = "outline_panel/search_updates.rs"]
mod search_updates;
#[path = "outline_panel/selection_navigation.rs"]
mod selection_navigation;
#[path = "outline_panel/selection_reveal.rs"]
mod selection_reveal;
#[path = "outline_panel/setup.rs"]
mod setup;

use anyhow::Context as _;
use collections::{BTreeSet, HashMap, HashSet};
use db::kvp::KeyValueStore;
use editor::{
    AnchorRangeExt, Bias, DisplayPoint, Editor, EditorEvent, ExcerptRange, MultiBufferSnapshot,
    RangeToAnchorExt, SelectionEffects,
    display_map::ToDisplayPoint,
    items::{entry_git_aware_label_color, entry_label_color},
    scroll::{Autoscroll, ScrollAnchor},
};
use file_icons::FileIcons;

use fuzzy::{StringMatch, StringMatchCandidate, match_strings};
use gpui::{
    Action, AnyElement, App, AppContext as _, AsyncWindowContext, Bounds, ClipboardItem, Context,
    DismissEvent, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement, IntoElement, KeyContext, ListHorizontalSizingBehavior, ListSizingBehavior,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render, ScrollStrategy,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled, Subscription, Task, TaskExt,
    UniformListScrollHandle, WeakEntity, Window, actions, anchored, deferred, div, point, px, size,
    uniform_list,
};
use itertools::Itertools;
use language::{Anchor, BufferId, BufferSnapshot, OffsetRangeExt, OutlineItem};
use language::{LanguageAwareStyling, language_settings::LanguageSettings};

use menu::{Cancel, SelectFirst, SelectLast, SelectNext, SelectPrevious};
use std::{
    cmp,
    collections::BTreeMap,
    hash::Hash,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{self, AtomicBool},
    },
    time::Duration,
    u32,
};

use outline_panel_settings::{DockSide, OutlinePanelSettings, ShowIndentGuides};
use panel_helpers::{
    back_to_common_visited_parent, empty_icon, file_name, find_active_indent_guide_ix,
    subscribe_for_editor_events,
};
use panel_runtime::workspace_active_editor;
use project::{File, Fs, GitEntry, GitTraversal, Project, ProjectItem};
use search::{BufferSearchBar, ProjectSearchView};
use search_state::*;
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsStore};
use theme::SyntaxTheme;
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, FluentBuilder, HighlightedLabel, IconButton, IconButtonShape, IndentGuideColors,
    IndentGuideLayout, KeyBinding, ListItem, ScrollAxes, Scrollbars, Tab, Tooltip, WithScrollbar,
    prelude::*,
};
use util::{RangeExt, ResultExt, TryFutureExt, debug_panic, rel_path::RelPath};
use workspace::{
    OpenInTerminal, WeakItemHandle, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    item::ItemHandle,
    searchable::{SearchEvent, SearchableItem},
};
use worktree::{Entry, ProjectEntryId, WorktreeId};

use crate::outline_panel_settings::OutlinePanelSettingsScrollbarProxy;
use cached_entry_helpers::GenerationState;
use entry_types::*;

actions!(
    outline_panel,
    [
        /// Collapses all entries in the outline tree.
        CollapseAllEntries,
        /// Collapses the currently selected entry.
        CollapseSelectedEntry,
        /// Expands all entries in the outline tree.
        ExpandAllEntries,
        /// Expands the currently selected entry.
        ExpandSelectedEntry,
        /// Folds the selected directory.
        FoldDirectory,
        /// Opens the selected entry in the editor.
        OpenSelectedEntry,
        /// Reveals the selected item in the system file manager.
        RevealInFileManager,
        /// Scroll half a page upwards
        ScrollUp,
        /// Scroll half a page downwards
        ScrollDown,
        /// Scroll until the cursor displays at the center
        ScrollCursorCenter,
        /// Scroll until the cursor displays at the top
        ScrollCursorTop,
        /// Scroll until the cursor displays at the bottom
        ScrollCursorBottom,
        /// Selects the parent of the current entry.
        SelectParent,
        /// Toggles the pin status of the active editor.
        ToggleActiveEditorPin,
        /// Unfolds the selected directory.
        UnfoldDirectory,
        /// Toggles the outline panel.
        Toggle,
        /// Toggles focus on the outline panel.
        ToggleFocus,
    ]
);

const OUTLINE_PANEL_KEY: &str = "OutlinePanel";
const UPDATE_DEBOUNCE: Duration = Duration::from_millis(50);

type Outline = OutlineItem<language::Anchor>;
type HighlightStyleData = Arc<OnceLock<Vec<(Range<usize>, HighlightStyle)>>>;

pub struct OutlinePanel {
    fs: Arc<dyn Fs>,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    active: bool,
    pinned: bool,
    scroll_handle: UniformListScrollHandle,
    rendered_entries_len: usize,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    focus_handle: FocusHandle,
    pending_serialization: Task<Option<()>>,
    fs_entries_depth: HashMap<(WorktreeId, ProjectEntryId), usize>,
    fs_entries: Vec<FsEntry>,
    fs_children_count: HashMap<WorktreeId, HashMap<Arc<RelPath>, FsChildren>>,
    collapsed_entries: HashSet<CollapsedEntry>,
    unfolded_dirs: HashMap<WorktreeId, BTreeSet<ProjectEntryId>>,
    selected_entry: SelectedEntry,
    active_item: Option<ActiveItem>,
    _subscriptions: Vec<Subscription>,
    new_entries_for_fs_update: HashSet<BufferId>,
    fs_entries_update_task: Task<()>,
    fs_entries_update_pending: bool,
    cached_entries_update_task: Task<()>,
    cached_entries_update_pending: bool,
    reveal_selection_task: Task<anyhow::Result<()>>,
    outline_fetch_tasks: HashMap<BufferId, Task<()>>,
    buffers: HashMap<BufferId, BufferOutlines>,
    cached_entries: Vec<CachedEntry>,
    filter_editor: Entity<Editor>,
    mode: ItemsDisplayMode,
    max_width_item_index: Option<usize>,
    preserve_selection_on_buffer_fold_toggles: HashSet<BufferId>,
    pending_default_expansion_depth: Option<usize>,
    outline_children_cache: HashMap<BufferId, HashMap<(Range<Anchor>, usize), bool>>,
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<OutlinePanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &Toggle, window, cx| {
            if !workspace.toggle_panel_focus::<OutlinePanel>(window, cx) {
                workspace.close_panel::<OutlinePanel>(window, cx);
            }
        });
    })
    .detach();
}

#[cfg(test)]
#[path = "outline_panel/tests.rs"]
mod tests;
