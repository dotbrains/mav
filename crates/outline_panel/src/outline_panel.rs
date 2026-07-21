#[path = "outline_panel/active_editor_state.rs"]
mod active_editor_state;
#[path = "outline_panel/buffer_outline_state.rs"]
mod buffer_outline_state;
#[path = "outline_panel/context_actions.rs"]
mod context_actions;
#[path = "outline_panel/entry_types.rs"]
mod entry_types;
#[path = "outline_panel/expand_collapse.rs"]
mod expand_collapse;
mod outline_panel_settings;
#[path = "outline_panel/panel_runtime.rs"]
mod panel_runtime;
#[path = "outline_panel/rendering.rs"]
mod rendering;
#[path = "outline_panel/search_state.rs"]
mod search_state;
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

impl OutlinePanel {
    fn update_fs_entries(
        &mut self,
        active_editor: Entity<Editor>,
        debounce: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }

        if debounce.is_some() && self.fs_entries_update_pending {
            return;
        }
        self.fs_entries_update_pending = true;

        self.fs_entries_update_task = cx.spawn_in(window, async move |outline_panel, cx| {
            if let Some(debounce) = debounce {
                cx.background_executor().timer(debounce).await;
            }

            let mut new_collapsed_entries = HashSet::default();
            let mut new_unfolded_dirs = HashMap::default();
            let mut root_entries = HashSet::default();
            let mut new_buffers = HashMap::<BufferId, BufferOutlines>::default();
            let Ok((buffer_excerpts, auto_fold_dirs, repo_snapshots)) =
                outline_panel.update(cx, |outline_panel, cx| {
                    outline_panel.fs_entries_update_pending = false;
                    let auto_fold_dirs = OutlinePanelSettings::get_global(cx).auto_fold_dirs;
                    let active_multi_buffer = active_editor.read(cx).buffer().clone();
                    let new_entries = outline_panel.new_entries_for_fs_update.clone();
                    let repo_snapshots = outline_panel.project.update(cx, |project, cx| {
                        project.git_store().read(cx).repo_snapshots(cx)
                    });
                    let git_store = outline_panel.project.read(cx).git_store().clone();
                    new_collapsed_entries = outline_panel.collapsed_entries.clone();
                    new_unfolded_dirs = outline_panel.unfolded_dirs.clone();
                    let multi_buffer_snapshot = active_multi_buffer.read(cx).snapshot(cx);

                    let buffer_excerpts = multi_buffer_snapshot.excerpts().fold(
                        HashMap::default(),
                        |mut buffer_excerpts, excerpt_range| {
                            let Some(buffer_snapshot) = multi_buffer_snapshot
                                .buffer_for_id(excerpt_range.context.start.buffer_id)
                            else {
                                return buffer_excerpts;
                            };
                            let buffer_id = buffer_snapshot.remote_id();
                            let file = File::from_dyn(buffer_snapshot.file());
                            let entry_id = file.and_then(|file| file.project_entry_id());
                            let worktree = file.map(|file| file.worktree.read(cx).snapshot());
                            let is_new = new_entries.contains(&buffer_id)
                                || !outline_panel.buffers.contains_key(&buffer_id);
                            let is_folded = active_editor.read(cx).is_buffer_folded(buffer_id, cx);
                            let status = git_store
                                .read(cx)
                                .repository_and_path_for_buffer_id(buffer_id, cx)
                                .and_then(|(repo, path)| {
                                    Some(repo.read(cx).status_for_path(&path)?.status)
                                });
                            buffer_excerpts
                                .entry(buffer_id)
                                .or_insert_with(|| {
                                    (is_new, is_folded, Vec::new(), entry_id, worktree, status)
                                })
                                .2
                                .push(excerpt_range.clone());

                            new_buffers
                                .entry(buffer_id)
                                .or_insert_with(|| {
                                    let outlines = match outline_panel.buffers.get(&buffer_id) {
                                        Some(old_buffer) => match &old_buffer.outlines {
                                            OutlineState::Outlines(outlines) => {
                                                OutlineState::Outlines(outlines.clone())
                                            }
                                            OutlineState::Invalidated(_) => {
                                                OutlineState::NotFetched
                                            }
                                            OutlineState::NotFetched => OutlineState::NotFetched,
                                        },
                                        None => OutlineState::NotFetched,
                                    };
                                    BufferOutlines {
                                        outlines,
                                        excerpts: Vec::new(),
                                    }
                                })
                                .excerpts
                                .push(excerpt_range);
                            buffer_excerpts
                        },
                    );
                    (buffer_excerpts, auto_fold_dirs, repo_snapshots)
                })
            else {
                return;
            };

            let Some((
                new_collapsed_entries,
                new_unfolded_dirs,
                new_fs_entries,
                new_depth_map,
                new_children_count,
            )) = cx
                .background_spawn(async move {
                    let mut processed_external_buffers = HashSet::default();
                    let mut new_worktree_entries =
                        BTreeMap::<WorktreeId, HashMap<ProjectEntryId, GitEntry>>::default();
                    let mut worktree_excerpts = HashMap::<
                        WorktreeId,
                        HashMap<ProjectEntryId, (BufferId, Vec<ExcerptRange<Anchor>>)>,
                    >::default();
                    let mut external_excerpts = HashMap::default();

                    for (buffer_id, (is_new, is_folded, excerpts, entry_id, worktree, status)) in
                        buffer_excerpts
                    {
                        if is_folded {
                            match &worktree {
                                Some(worktree) => {
                                    new_collapsed_entries
                                        .insert(CollapsedEntry::File(worktree.id(), buffer_id));
                                }
                                None => {
                                    new_collapsed_entries
                                        .insert(CollapsedEntry::ExternalFile(buffer_id));
                                }
                            }
                        } else if is_new {
                            match &worktree {
                                Some(worktree) => {
                                    new_collapsed_entries
                                        .remove(&CollapsedEntry::File(worktree.id(), buffer_id));
                                }
                                None => {
                                    new_collapsed_entries
                                        .remove(&CollapsedEntry::ExternalFile(buffer_id));
                                }
                            }
                        }

                        if let Some(worktree) = worktree {
                            let worktree_id = worktree.id();
                            let unfolded_dirs = new_unfolded_dirs.entry(worktree_id).or_default();

                            match entry_id.and_then(|id| worktree.entry_for_id(id)).cloned() {
                                Some(entry) => {
                                    let entry = GitEntry {
                                        git_summary: status
                                            .map(|status| status.summary())
                                            .unwrap_or_default(),
                                        entry,
                                    };
                                    let mut traversal = GitTraversal::new(
                                        &repo_snapshots,
                                        worktree.traverse_from_path(
                                            true,
                                            true,
                                            true,
                                            entry.path.as_ref(),
                                        ),
                                    );

                                    let mut entries_to_add = HashMap::default();
                                    worktree_excerpts
                                        .entry(worktree_id)
                                        .or_default()
                                        .insert(entry.id, (buffer_id, excerpts));
                                    let mut current_entry = entry;
                                    loop {
                                        if current_entry.is_dir() {
                                            let is_root =
                                                worktree.root_entry().map(|entry| entry.id)
                                                    == Some(current_entry.id);
                                            if is_root {
                                                root_entries.insert(current_entry.id);
                                                if auto_fold_dirs {
                                                    unfolded_dirs.insert(current_entry.id);
                                                }
                                            }
                                            if is_new {
                                                new_collapsed_entries.remove(&CollapsedEntry::Dir(
                                                    worktree_id,
                                                    current_entry.id,
                                                ));
                                            }
                                        }

                                        let new_entry_added = entries_to_add
                                            .insert(current_entry.id, current_entry)
                                            .is_none();
                                        if new_entry_added
                                            && traversal.back_to_parent()
                                            && let Some(parent_entry) = traversal.entry()
                                        {
                                            current_entry = parent_entry.to_owned();
                                            continue;
                                        }
                                        break;
                                    }
                                    new_worktree_entries
                                        .entry(worktree_id)
                                        .or_insert_with(HashMap::default)
                                        .extend(entries_to_add);
                                }
                                None => {
                                    if processed_external_buffers.insert(buffer_id) {
                                        external_excerpts
                                            .entry(buffer_id)
                                            .or_insert_with(Vec::new)
                                            .extend(excerpts);
                                    }
                                }
                            }
                        } else if processed_external_buffers.insert(buffer_id) {
                            external_excerpts
                                .entry(buffer_id)
                                .or_insert_with(Vec::new)
                                .extend(excerpts);
                        }
                    }

                    let mut new_children_count =
                        HashMap::<WorktreeId, HashMap<Arc<RelPath>, FsChildren>>::default();

                    let worktree_entries = new_worktree_entries
                        .into_iter()
                        .map(|(worktree_id, entries)| {
                            let mut entries = entries.into_values().collect::<Vec<_>>();
                            entries.sort_by(|a, b| a.path.as_ref().cmp(b.path.as_ref()));
                            (worktree_id, entries)
                        })
                        .flat_map(|(worktree_id, entries)| {
                            {
                                entries
                                    .into_iter()
                                    .filter_map(|entry| {
                                        if auto_fold_dirs && let Some(parent) = entry.path.parent()
                                        {
                                            let children = new_children_count
                                                .entry(worktree_id)
                                                .or_default()
                                                .entry(Arc::from(parent))
                                                .or_default();
                                            if entry.is_dir() {
                                                children.dirs += 1;
                                            } else {
                                                children.files += 1;
                                            }
                                        }

                                        if entry.is_dir() {
                                            Some(FsEntry::Directory(FsEntryDirectory {
                                                worktree_id,
                                                entry,
                                            }))
                                        } else {
                                            let (buffer_id, excerpts) = worktree_excerpts
                                                .get_mut(&worktree_id)
                                                .and_then(|worktree_excerpts| {
                                                    worktree_excerpts.remove(&entry.id)
                                                })?;
                                            Some(FsEntry::File(FsEntryFile {
                                                worktree_id,
                                                buffer_id,
                                                entry,
                                                excerpts,
                                            }))
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            }
                        })
                        .collect::<Vec<_>>();

                    let mut visited_dirs = Vec::new();
                    let mut new_depth_map = HashMap::default();
                    let new_visible_entries = external_excerpts
                        .into_iter()
                        .sorted_by_key(|(id, _)| *id)
                        .map(|(buffer_id, excerpts)| {
                            FsEntry::ExternalFile(FsEntryExternalFile {
                                buffer_id,
                                excerpts,
                            })
                        })
                        .chain(worktree_entries)
                        .filter(|visible_item| {
                            match visible_item {
                                FsEntry::Directory(directory) => {
                                    let parent_id = back_to_common_visited_parent(
                                        &mut visited_dirs,
                                        &directory.worktree_id,
                                        &directory.entry,
                                    );

                                    let mut depth = 0;
                                    if !root_entries.contains(&directory.entry.id) {
                                        if auto_fold_dirs {
                                            let children = new_children_count
                                                .get(&directory.worktree_id)
                                                .and_then(|children_count| {
                                                    children_count.get(&directory.entry.path)
                                                })
                                                .copied()
                                                .unwrap_or_default();

                                            if !children.may_be_fold_part()
                                                || (children.dirs == 0
                                                    && visited_dirs
                                                        .last()
                                                        .map(|(parent_dir_id, _)| {
                                                            new_unfolded_dirs
                                                                .get(&directory.worktree_id)
                                                                .is_none_or(|unfolded_dirs| {
                                                                    unfolded_dirs
                                                                        .contains(parent_dir_id)
                                                                })
                                                        })
                                                        .unwrap_or(true))
                                            {
                                                new_unfolded_dirs
                                                    .entry(directory.worktree_id)
                                                    .or_default()
                                                    .insert(directory.entry.id);
                                            }
                                        }

                                        depth = parent_id
                                            .and_then(|(worktree_id, id)| {
                                                new_depth_map.get(&(worktree_id, id)).copied()
                                            })
                                            .unwrap_or(0)
                                            + 1;
                                    };
                                    visited_dirs
                                        .push((directory.entry.id, directory.entry.path.clone()));
                                    new_depth_map
                                        .insert((directory.worktree_id, directory.entry.id), depth);
                                }
                                FsEntry::File(FsEntryFile {
                                    worktree_id,
                                    entry: file_entry,
                                    ..
                                }) => {
                                    let parent_id = back_to_common_visited_parent(
                                        &mut visited_dirs,
                                        worktree_id,
                                        file_entry,
                                    );
                                    let depth = if root_entries.contains(&file_entry.id) {
                                        0
                                    } else {
                                        parent_id
                                            .and_then(|(worktree_id, id)| {
                                                new_depth_map.get(&(worktree_id, id)).copied()
                                            })
                                            .unwrap_or(0)
                                            + 1
                                    };
                                    new_depth_map.insert((*worktree_id, file_entry.id), depth);
                                }
                                FsEntry::ExternalFile(..) => {
                                    visited_dirs.clear();
                                }
                            }

                            true
                        })
                        .collect::<Vec<_>>();

                    anyhow::Ok((
                        new_collapsed_entries,
                        new_unfolded_dirs,
                        new_visible_entries,
                        new_depth_map,
                        new_children_count,
                    ))
                })
                .await
                .log_err()
            else {
                return;
            };

            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.new_entries_for_fs_update.clear();
                    outline_panel.buffers = new_buffers;
                    outline_panel.collapsed_entries = new_collapsed_entries;
                    outline_panel.unfolded_dirs = new_unfolded_dirs;
                    outline_panel.fs_entries = new_fs_entries;
                    outline_panel.fs_entries_depth = new_depth_map;
                    outline_panel.fs_children_count = new_children_count;
                    outline_panel.update_non_fs_items(window, cx);

                    // Only update cached entries if we don't have outlines to fetch
                    // If we do have outlines to fetch, let fetch_outdated_outlines handle the update
                    if outline_panel.buffers_to_fetch().is_empty() {
                        outline_panel.update_cached_entries(debounce, window, cx);
                    }

                    cx.notify();
                })
                .ok();
        });
    }

    fn update_cached_entries(
        &mut self,
        debounce: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) {
        if !self.active {
            return;
        }

        // A pending debounced update will read the latest state when it fires,
        // so we don't need to reschedule. Constantly rescheduling under a steady stream
        // of events (e.g. project search streaming results) would starve the task forever.
        if debounce.is_some() && self.cached_entries_update_pending {
            return;
        }
        self.cached_entries_update_pending = true;

        self.cached_entries_update_task = cx.spawn_in(window, async move |outline_panel, cx| {
            if let Some(debounce) = debounce {
                cx.background_executor().timer(debounce).await;
            }
            let Some(new_cached_entries) = outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.cached_entries_update_pending = false;
                    let is_singleton = outline_panel.is_singleton_active(cx);
                    let query = outline_panel.query(cx);
                    outline_panel.generate_cached_entries(is_singleton, query, window, cx)
                })
                .ok()
            else {
                return;
            };
            let (new_cached_entries, max_width_item_index) = new_cached_entries.await;
            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.cached_entries = new_cached_entries;
                    outline_panel.max_width_item_index = max_width_item_index;
                    if (outline_panel.selected_entry.is_invalidated()
                        || matches!(outline_panel.selected_entry, SelectedEntry::None))
                        && let Some(new_selected_entry) =
                            outline_panel.active_editor().and_then(|active_editor| {
                                outline_panel.location_for_editor_selection(
                                    &active_editor,
                                    window,
                                    cx,
                                )
                            })
                    {
                        outline_panel.select_entry(new_selected_entry, false, window, cx);
                    }

                    cx.notify();
                })
                .ok();
        });
    }

    fn generate_cached_entries(
        &self,
        is_singleton: bool,
        query: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<(Vec<CachedEntry>, Option<usize>)> {
        let project = self.project.clone();
        let Some(active_editor) = self.active_editor() else {
            return Task::ready((Vec::new(), None));
        };
        cx.spawn_in(window, async move |outline_panel, cx| {
            let mut generation_state = GenerationState::default();

            let Ok(()) = outline_panel.update(cx, |outline_panel, cx| {
                let auto_fold_dirs = OutlinePanelSettings::get_global(cx).auto_fold_dirs;
                let mut folded_dirs_entry = None::<(usize, FoldedDirsEntry)>;
                let track_matches = query.is_some();

                #[derive(Debug)]
                struct ParentStats {
                    path: Arc<RelPath>,
                    folded: bool,
                    expanded: bool,
                    depth: usize,
                }

                let search_precomputed =
                    if let ItemsDisplayMode::Search(search_state) = &outline_panel.mode {
                        let multi_buffer_snapshot =
                            active_editor.read(cx).buffer().read(cx).snapshot(cx);
                        let mut folded_buffers = HashSet::default();
                        let mut not_folded_buffers = HashSet::default();
                        let mut matches_by_buffer = HashMap::default();

                        for (match_range, search_data) in &search_state.matches {
                            let Some((start_anchor, _)) =
                                multi_buffer_snapshot.anchor_to_buffer_anchor(match_range.start)
                            else {
                                continue;
                            };
                            let start_buffer_id = start_anchor.buffer_id;
                            let end_buffer_id = multi_buffer_snapshot
                                .anchor_to_buffer_anchor(match_range.end)
                                .map(|(anchor, _)| anchor.buffer_id);

                            let mut any_folded = false;
                            for buffer_id in
                                [Some(start_buffer_id), end_buffer_id].into_iter().flatten()
                            {
                                if folded_buffers.contains(&buffer_id) {
                                    any_folded = true;
                                } else if !not_folded_buffers.contains(&buffer_id) {
                                    if active_editor.read(cx).is_buffer_folded(buffer_id, cx) {
                                        folded_buffers.insert(buffer_id);
                                        any_folded = true;
                                    } else {
                                        not_folded_buffers.insert(buffer_id);
                                    }
                                }
                            }
                            if any_folded {
                                continue;
                            }

                            matches_by_buffer
                                .entry(start_buffer_id)
                                .or_insert_with(Vec::new)
                                .push((match_range.clone(), Arc::clone(search_data)));
                        }

                        Some(SearchPrecomputed {
                            multi_buffer_snapshot,
                            matches_by_buffer,
                            folded_buffers,
                        })
                    } else {
                        None
                    };

                let mut parent_dirs = Vec::<ParentStats>::new();
                for entry in outline_panel.fs_entries.clone() {
                    let is_expanded = outline_panel.is_expanded(&entry);
                    let (depth, should_add) = match &entry {
                        FsEntry::Directory(directory_entry) => {
                            let mut should_add = true;
                            let is_root = project
                                .read(cx)
                                .worktree_for_id(directory_entry.worktree_id, cx)
                                .is_some_and(|worktree| {
                                    worktree.read(cx).root_entry() == Some(&directory_entry.entry)
                                });
                            let folded = auto_fold_dirs
                                && !is_root
                                && outline_panel
                                    .unfolded_dirs
                                    .get(&directory_entry.worktree_id)
                                    .is_none_or(|unfolded_dirs| {
                                        !unfolded_dirs.contains(&directory_entry.entry.id)
                                    });
                            let fs_depth = outline_panel
                                .fs_entries_depth
                                .get(&(directory_entry.worktree_id, directory_entry.entry.id))
                                .copied()
                                .unwrap_or(0);
                            while let Some(parent) = parent_dirs.last() {
                                if !is_root && directory_entry.entry.path.starts_with(&parent.path)
                                {
                                    break;
                                }
                                parent_dirs.pop();
                            }
                            let auto_fold = match parent_dirs.last() {
                                Some(parent) => {
                                    parent.folded
                                        && Some(parent.path.as_ref())
                                            == directory_entry.entry.path.parent()
                                        && outline_panel
                                            .fs_children_count
                                            .get(&directory_entry.worktree_id)
                                            .and_then(|entries| {
                                                entries.get(&directory_entry.entry.path)
                                            })
                                            .copied()
                                            .unwrap_or_default()
                                            .may_be_fold_part()
                                }
                                None => false,
                            };
                            let folded = folded || auto_fold;
                            let (depth, parent_expanded, parent_folded) = match parent_dirs.last() {
                                Some(parent) => {
                                    let parent_folded = parent.folded;
                                    let parent_expanded = parent.expanded;
                                    let new_depth = if parent_folded {
                                        parent.depth
                                    } else {
                                        parent.depth + 1
                                    };
                                    parent_dirs.push(ParentStats {
                                        path: directory_entry.entry.path.clone(),
                                        folded,
                                        expanded: parent_expanded && is_expanded,
                                        depth: new_depth,
                                    });
                                    (new_depth, parent_expanded, parent_folded)
                                }
                                None => {
                                    parent_dirs.push(ParentStats {
                                        path: directory_entry.entry.path.clone(),
                                        folded,
                                        expanded: is_expanded,
                                        depth: fs_depth,
                                    });
                                    (fs_depth, true, false)
                                }
                            };

                            if let Some((folded_depth, mut folded_dirs)) = folded_dirs_entry.take()
                            {
                                if folded
                                    && directory_entry.worktree_id == folded_dirs.worktree_id
                                    && directory_entry.entry.path.parent()
                                        == folded_dirs
                                            .entries
                                            .last()
                                            .map(|entry| entry.path.as_ref())
                                {
                                    folded_dirs.entries.push(directory_entry.entry.clone());
                                    folded_dirs_entry = Some((folded_depth, folded_dirs))
                                } else {
                                    if !is_singleton {
                                        let start_of_collapsed_dir_sequence = !parent_expanded
                                            && parent_dirs
                                                .iter()
                                                .rev()
                                                .nth(folded_dirs.entries.len() + 1)
                                                .is_none_or(|parent| parent.expanded);
                                        if start_of_collapsed_dir_sequence
                                            || parent_expanded
                                            || query.is_some()
                                        {
                                            if parent_folded {
                                                folded_dirs
                                                    .entries
                                                    .push(directory_entry.entry.clone());
                                                should_add = false;
                                            }
                                            let new_folded_dirs =
                                                PanelEntry::FoldedDirs(folded_dirs.clone());
                                            outline_panel.push_entry(
                                                &mut generation_state,
                                                track_matches,
                                                new_folded_dirs,
                                                folded_depth,
                                                cx,
                                            );
                                        }
                                    }

                                    folded_dirs_entry = if parent_folded {
                                        None
                                    } else {
                                        Some((
                                            depth,
                                            FoldedDirsEntry {
                                                worktree_id: directory_entry.worktree_id,
                                                entries: vec![directory_entry.entry.clone()],
                                            },
                                        ))
                                    };
                                }
                            } else if folded {
                                folded_dirs_entry = Some((
                                    depth,
                                    FoldedDirsEntry {
                                        worktree_id: directory_entry.worktree_id,
                                        entries: vec![directory_entry.entry.clone()],
                                    },
                                ));
                            }

                            let should_add =
                                should_add && parent_expanded && folded_dirs_entry.is_none();
                            (depth, should_add)
                        }
                        FsEntry::ExternalFile(..) => {
                            if let Some((folded_depth, folded_dir)) = folded_dirs_entry.take() {
                                let parent_expanded = parent_dirs
                                    .iter()
                                    .rev()
                                    .find(|parent| {
                                        folded_dir
                                            .entries
                                            .iter()
                                            .all(|entry| entry.path != parent.path)
                                    })
                                    .is_none_or(|parent| parent.expanded);
                                if !is_singleton && (parent_expanded || query.is_some()) {
                                    outline_panel.push_entry(
                                        &mut generation_state,
                                        track_matches,
                                        PanelEntry::FoldedDirs(folded_dir),
                                        folded_depth,
                                        cx,
                                    );
                                }
                            }
                            parent_dirs.clear();
                            (0, true)
                        }
                        FsEntry::File(file) => {
                            if let Some((folded_depth, folded_dirs)) = folded_dirs_entry.take() {
                                let parent_expanded = parent_dirs
                                    .iter()
                                    .rev()
                                    .find(|parent| {
                                        folded_dirs
                                            .entries
                                            .iter()
                                            .all(|entry| entry.path != parent.path)
                                    })
                                    .is_none_or(|parent| parent.expanded);
                                if !is_singleton && (parent_expanded || query.is_some()) {
                                    outline_panel.push_entry(
                                        &mut generation_state,
                                        track_matches,
                                        PanelEntry::FoldedDirs(folded_dirs),
                                        folded_depth,
                                        cx,
                                    );
                                }
                            }

                            let fs_depth = outline_panel
                                .fs_entries_depth
                                .get(&(file.worktree_id, file.entry.id))
                                .copied()
                                .unwrap_or(0);
                            while let Some(parent) = parent_dirs.last() {
                                if file.entry.path.starts_with(&parent.path) {
                                    break;
                                }
                                parent_dirs.pop();
                            }
                            match parent_dirs.last() {
                                Some(parent) => {
                                    let new_depth = parent.depth + 1;
                                    (new_depth, parent.expanded)
                                }
                                None => (fs_depth, true),
                            }
                        }
                    };

                    if !is_singleton
                        && (should_add || (query.is_some() && folded_dirs_entry.is_none()))
                    {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::Fs(entry.clone()),
                            depth,
                            cx,
                        );
                    }

                    match outline_panel.mode {
                        ItemsDisplayMode::Search(_) => {
                            if (is_singleton || query.is_some() || (should_add && is_expanded))
                                && let Some(search) = &search_precomputed
                            {
                                outline_panel.add_search_entries(
                                    &mut generation_state,
                                    search,
                                    &entry,
                                    depth,
                                    query.is_some(),
                                    is_singleton,
                                    cx,
                                );
                            }
                        }
                        ItemsDisplayMode::Outline => {
                            let excerpts_to_consider =
                                if is_singleton || query.is_some() || (should_add && is_expanded) {
                                    match &entry {
                                        FsEntry::File(FsEntryFile {
                                            buffer_id,
                                            excerpts,
                                            ..
                                        })
                                        | FsEntry::ExternalFile(FsEntryExternalFile {
                                            buffer_id,
                                            excerpts,
                                            ..
                                        }) => Some((*buffer_id, excerpts)),
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                            if let Some((buffer_id, _entry_excerpts)) = excerpts_to_consider
                                && !active_editor.read(cx).is_buffer_folded(buffer_id, cx)
                            {
                                outline_panel.add_buffer_entries(
                                    &mut generation_state,
                                    buffer_id,
                                    depth,
                                    track_matches,
                                    is_singleton,
                                    query.as_deref(),
                                    cx,
                                );
                            }
                        }
                    }

                    if is_singleton
                        && matches!(entry, FsEntry::File(..) | FsEntry::ExternalFile(..))
                        && !generation_state.entries.iter().any(|item| {
                            matches!(item.entry, PanelEntry::Outline(..) | PanelEntry::Search(_))
                        })
                    {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::Fs(entry.clone()),
                            0,
                            cx,
                        );
                    }
                }

                if let Some((folded_depth, folded_dirs)) = folded_dirs_entry.take() {
                    let parent_expanded = parent_dirs
                        .iter()
                        .rev()
                        .find(|parent| {
                            folded_dirs
                                .entries
                                .iter()
                                .all(|entry| entry.path != parent.path)
                        })
                        .is_none_or(|parent| parent.expanded);
                    if parent_expanded || query.is_some() {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::FoldedDirs(folded_dirs),
                            folded_depth,
                            cx,
                        );
                    }
                }
            }) else {
                return (Vec::new(), None);
            };

            let Some(query) = query else {
                return (
                    generation_state.entries,
                    generation_state
                        .max_width_estimate_and_index
                        .map(|(_, index)| index),
                );
            };

            let mut matched_ids = match_strings(
                &generation_state.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &AtomicBool::default(),
                cx.background_executor().clone(),
            )
            .await
            .into_iter()
            .map(|string_match| (string_match.candidate_id, string_match))
            .collect::<HashMap<_, _>>();

            let mut id = 0;
            generation_state.entries.retain_mut(|cached_entry| {
                let retain = match matched_ids.remove(&id) {
                    Some(string_match) => {
                        cached_entry.string_match = Some(string_match);
                        true
                    }
                    None => false,
                };
                id += 1;
                retain
            });

            (
                generation_state.entries,
                generation_state
                    .max_width_estimate_and_index
                    .map(|(_, index)| index),
            )
        })
    }

    fn push_entry(
        &self,
        state: &mut GenerationState,
        track_matches: bool,
        entry: PanelEntry,
        depth: usize,
        cx: &mut App,
    ) {
        let entry = if let PanelEntry::FoldedDirs(folded_dirs_entry) = &entry {
            match folded_dirs_entry.entries.len() {
                0 => {
                    debug_panic!("Empty folded dirs receiver");
                    return;
                }
                1 => PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                    worktree_id: folded_dirs_entry.worktree_id,
                    entry: folded_dirs_entry.entries[0].clone(),
                })),
                _ => entry,
            }
        } else {
            entry
        };

        if track_matches {
            let id = state.entries.len();
            match &entry {
                PanelEntry::Fs(fs_entry) => {
                    if let Some(file_name) = self
                        .relative_path(fs_entry, cx)
                        .and_then(|path| Some(path.file_name()?.to_string()))
                    {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &file_name));
                    }
                }
                PanelEntry::FoldedDirs(folded_dir_entry) => {
                    let dir_names = self.dir_names_string(
                        &folded_dir_entry.entries,
                        folded_dir_entry.worktree_id,
                        cx,
                    );
                    {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &dir_names));
                    }
                }
                PanelEntry::Outline(OutlineEntry::Outline(outline_entry)) => state
                    .match_candidates
                    .push(StringMatchCandidate::new(id, &outline_entry.text)),
                PanelEntry::Outline(OutlineEntry::Excerpt(_)) => {}
                PanelEntry::Search(new_search_entry) => {
                    if let Some(search_data) = new_search_entry.render_data.get() {
                        state
                            .match_candidates
                            .push(StringMatchCandidate::new(id, &search_data.context_text));
                    }
                }
            }
        }

        let width_estimate = self.width_estimate(depth, &entry, cx);
        if Some(width_estimate)
            > state
                .max_width_estimate_and_index
                .map(|(estimate, _)| estimate)
        {
            state.max_width_estimate_and_index = Some((width_estimate, state.entries.len()));
        }
        state.entries.push(CachedEntry {
            depth,
            entry,
            string_match: None,
        });
    }

    fn dir_names_string(&self, entries: &[GitEntry], worktree_id: WorktreeId, cx: &App) -> String {
        let dir_names_segment = entries
            .iter()
            .map(|entry| self.entry_name(&worktree_id, entry, cx))
            .collect::<PathBuf>();
        dir_names_segment.to_string_lossy().into_owned()
    }

    fn query(&self, cx: &App) -> Option<String> {
        let query = self.filter_editor.read(cx).text(cx);
        if query.trim().is_empty() {
            None
        } else {
            Some(query)
        }
    }

    fn is_expanded(&self, entry: &FsEntry) -> bool {
        let entry_to_check = match entry {
            FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }) => {
                CollapsedEntry::ExternalFile(*buffer_id)
            }
            FsEntry::File(FsEntryFile {
                worktree_id,
                buffer_id,
                ..
            }) => CollapsedEntry::File(*worktree_id, *buffer_id),
            FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            }) => CollapsedEntry::Dir(*worktree_id, entry.id),
        };
        !self.collapsed_entries.contains(&entry_to_check)
    }

    fn update_non_fs_items(&mut self, window: &mut Window, cx: &mut Context<OutlinePanel>) -> bool {
        if !self.active {
            return false;
        }

        let mut update_cached_items = false;
        update_cached_items |= self.update_search_matches(window, cx);
        self.fetch_outdated_outlines(window, cx);
        if update_cached_items {
            self.selected_entry.invalidate();
        }
        update_cached_items
    }

    fn update_search_matches(
        &mut self,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) -> bool {
        if !self.active {
            return false;
        }

        let project_search = self
            .active_item()
            .and_then(|item| item.downcast::<ProjectSearchView>());
        let project_search_matches = project_search
            .as_ref()
            .map(|project_search| project_search.read(cx).get_matches(cx))
            .unwrap_or_default();

        let buffer_search = self
            .active_item()
            .as_deref()
            .and_then(|active_item| {
                self.workspace
                    .upgrade()
                    .and_then(|workspace| workspace.read(cx).pane_for(active_item))
            })
            .and_then(|pane| {
                pane.read(cx)
                    .toolbar()
                    .read(cx)
                    .item_of_type::<BufferSearchBar>()
            });
        let buffer_search_matches = self
            .active_editor()
            .map(|active_editor| {
                active_editor.update(cx, |editor, cx| editor.get_matches(window, cx).0)
            })
            .unwrap_or_default();

        let mut update_cached_entries = false;
        if buffer_search_matches.is_empty() && project_search_matches.is_empty() {
            if matches!(self.mode, ItemsDisplayMode::Search(_)) {
                self.mode = ItemsDisplayMode::Outline;
                update_cached_entries = true;
            }
        } else {
            let (kind, new_search_matches, new_search_query) = if buffer_search_matches.is_empty() {
                (
                    SearchKind::Project,
                    project_search_matches,
                    project_search
                        .map(|project_search| project_search.read(cx).search_query_text(cx))
                        .unwrap_or_default(),
                )
            } else {
                (
                    SearchKind::Buffer,
                    buffer_search_matches,
                    buffer_search
                        .map(|buffer_search| buffer_search.read(cx).query(cx))
                        .unwrap_or_default(),
                )
            };

            let changed = match &self.mode {
                ItemsDisplayMode::Search(current) => {
                    current.query != new_search_query
                        || current.kind != kind
                        || current.matches.len() != new_search_matches.len()
                        || current
                            .matches
                            .iter()
                            .zip(&new_search_matches)
                            .any(|((existing, _), incoming)| existing != incoming)
                }
                ItemsDisplayMode::Outline => true,
            };
            if changed {
                let previous_matches = match &mut self.mode {
                    ItemsDisplayMode::Search(current) if current.kind == kind => {
                        current.matches.drain(..).collect()
                    }
                    _ => HashMap::default(),
                };
                self.mode = ItemsDisplayMode::Search(SearchState::new(
                    kind,
                    new_search_query,
                    previous_matches,
                    new_search_matches,
                    cx.theme().syntax().clone(),
                    window,
                    cx,
                ));
                update_cached_entries = true;
            }
        }
        update_cached_entries
    }

    fn add_buffer_entries(
        &mut self,
        state: &mut GenerationState,
        buffer_id: BufferId,
        parent_depth: usize,
        track_matches: bool,
        is_singleton: bool,
        query: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.buffers.get(&buffer_id) else {
            return;
        };

        let buffer_snapshot = self.buffer_snapshot_for_id(buffer_id, cx);

        for excerpt in &buffer.excerpts {
            let excerpt_depth = parent_depth + 1;
            self.push_entry(
                state,
                track_matches,
                PanelEntry::Outline(OutlineEntry::Excerpt(excerpt.clone())),
                excerpt_depth,
                cx,
            );

            let mut outline_base_depth = excerpt_depth + 1;
            if is_singleton {
                outline_base_depth = 0;
                state.clear();
            } else if query.is_none()
                && self
                    .collapsed_entries
                    .contains(&CollapsedEntry::Excerpt(excerpt.clone()))
            {
                continue;
            }

            let mut last_depth_at_level: Vec<Option<Range<Anchor>>> = vec![None; 10];

            let all_outlines: Vec<_> = buffer.iter_outlines().collect();

            let mut outline_has_children = HashMap::default();
            let mut visible_outlines = Vec::new();
            let mut collapsed_state: Option<(usize, Range<Anchor>)> = None;

            for (i, &outline) in all_outlines.iter().enumerate() {
                let has_children = all_outlines
                    .get(i + 1)
                    .map(|next| next.depth > outline.depth)
                    .unwrap_or(false);

                outline_has_children.insert((outline.range.clone(), outline.depth), has_children);

                let mut should_include = true;

                if let Some((collapsed_depth, collapsed_range)) = &collapsed_state {
                    if outline.depth <= *collapsed_depth {
                        collapsed_state = None;
                    } else if let Some(buffer_snapshot) = buffer_snapshot.as_ref() {
                        let outline_start = outline.range.start;
                        if outline_start
                            .cmp(&collapsed_range.start, buffer_snapshot)
                            .is_ge()
                            && outline_start
                                .cmp(&collapsed_range.end, buffer_snapshot)
                                .is_lt()
                        {
                            should_include = false; // Skip - inside collapsed range
                        } else {
                            collapsed_state = None;
                        }
                    }
                }

                // Check if this outline itself is collapsed
                if should_include
                    && self
                        .collapsed_entries
                        .contains(&CollapsedEntry::Outline(outline.range.clone()))
                {
                    collapsed_state = Some((outline.depth, outline.range.clone()));
                }

                if should_include {
                    visible_outlines.push(outline);
                }
            }

            self.outline_children_cache
                .entry(buffer_id)
                .or_default()
                .extend(outline_has_children);

            for outline in visible_outlines {
                let outline_entry = outline.clone();

                if outline.depth < last_depth_at_level.len() {
                    last_depth_at_level[outline.depth] = Some(outline.range.clone());
                    // Clear deeper levels when we go back to a shallower depth
                    for d in (outline.depth + 1)..last_depth_at_level.len() {
                        last_depth_at_level[d] = None;
                    }
                }

                self.push_entry(
                    state,
                    track_matches,
                    PanelEntry::Outline(OutlineEntry::Outline(outline_entry)),
                    outline_base_depth + outline.depth,
                    cx,
                );
            }
        }
    }

    fn add_search_entries(
        &mut self,
        state: &mut GenerationState,
        search: &SearchPrecomputed,
        parent_entry: &FsEntry,
        parent_depth: usize,
        track_matches: bool,
        is_singleton: bool,
        cx: &mut Context<Self>,
    ) {
        let ItemsDisplayMode::Search(search_state) = &self.mode else {
            return;
        };
        let kind = search_state.kind;

        let (buffer_id, excerpts) = match parent_entry {
            FsEntry::Directory(_) => return,
            FsEntry::ExternalFile(external) => (external.buffer_id, &external.excerpts),
            FsEntry::File(file) => (file.buffer_id, &file.excerpts),
        };

        if search.folded_buffers.contains(&buffer_id) {
            return;
        }
        let Some(buffer_matches) = search.matches_by_buffer.get(&buffer_id) else {
            return;
        };

        let excerpt_ranges = excerpts
            .iter()
            .filter_map(|excerpt| {
                let start = search
                    .multi_buffer_snapshot
                    .anchor_in_buffer(excerpt.context.start)?;
                let end = search
                    .multi_buffer_snapshot
                    .anchor_in_buffer(excerpt.context.end)?;
                Some(start..end)
            })
            .collect::<Vec<_>>();

        let depth = if is_singleton { 0 } else { parent_depth + 1 };
        for (match_range, search_data) in buffer_matches.iter().filter(|(match_range, _)| {
            excerpt_ranges.iter().any(|excerpt_range| {
                excerpt_range.overlaps(match_range, &search.multi_buffer_snapshot)
            })
        }) {
            self.push_entry(
                state,
                track_matches,
                PanelEntry::Search(SearchEntry {
                    match_range: match_range.clone(),
                    kind,
                    render_data: Arc::clone(search_data),
                }),
                depth,
                cx,
            );
        }
    }

    fn selected_entry(&self) -> Option<&PanelEntry> {
        match &self.selected_entry {
            SelectedEntry::Invalidated(entry) => entry.as_ref(),
            SelectedEntry::Valid(entry, _) => Some(entry),
            SelectedEntry::None => None,
        }
    }

    fn select_entry(
        &mut self,
        entry: PanelEntry,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if focus {
            self.focus_handle.focus(window, cx);
        }
        let ix = self
            .cached_entries
            .iter()
            .enumerate()
            .find(|(_, cached_entry)| &cached_entry.entry == &entry)
            .map(|(i, _)| i)
            .unwrap_or_default();

        self.selected_entry = SelectedEntry::Valid(entry, ix);

        self.autoscroll(cx);
        cx.notify();
    }

    fn width_estimate(&self, depth: usize, entry: &PanelEntry, cx: &App) -> u64 {
        let item_text_chars = match entry {
            PanelEntry::Fs(FsEntry::ExternalFile(external)) => self
                .buffer_snapshot_for_id(external.buffer_id, cx)
                .and_then(|snapshot| Some(snapshot.file()?.path().file_name()?.len()))
                .unwrap_or_default(),
            PanelEntry::Fs(FsEntry::Directory(directory)) => directory
                .entry
                .path
                .file_name()
                .map(|name| name.len())
                .unwrap_or_default(),
            PanelEntry::Fs(FsEntry::File(file)) => file
                .entry
                .path
                .file_name()
                .map(|name| name.len())
                .unwrap_or_default(),
            PanelEntry::FoldedDirs(folded_dirs) => {
                folded_dirs
                    .entries
                    .iter()
                    .map(|dir| {
                        dir.path
                            .file_name()
                            .map(|name| name.len())
                            .unwrap_or_default()
                    })
                    .sum::<usize>()
                    + folded_dirs.entries.len().saturating_sub(1) * "/".len()
            }
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => self
                .excerpt_label(&excerpt, cx)
                .map(|label| label.len())
                .unwrap_or_default(),
            PanelEntry::Outline(OutlineEntry::Outline(entry)) => entry.text.len(),
            PanelEntry::Search(search) => search
                .render_data
                .get()
                .map(|data| data.context_text.len())
                .unwrap_or_default(),
        };

        (item_text_chars + depth) as u64
    }

    fn render_main_contents(
        &mut self,
        query: Option<String>,
        show_indent_guides: bool,
        indent_size: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let contents = if self.cached_entries.is_empty() {
            let header = if query.is_some() {
                "No matches for query"
            } else {
                "No outlines available"
            };

            v_flex()
                .id("empty-outline-state")
                .gap_0p5()
                .flex_1()
                .justify_center()
                .size_full()
                .child(h_flex().justify_center().child(Label::new(header)))
                .when_some(query, |panel, query| {
                    panel.child(
                        h_flex()
                            .px_0p5()
                            .justify_center()
                            .bg(cx.theme().colors().element_selected.opacity(0.2))
                            .child(Label::new(query)),
                    )
                })
                .child(
                    h_flex()
                        .gap_1()
                        .justify_center()
                        .child(Label::new("Toggle Panel With").color(Color::Muted))
                        .when_some(
                            match self.position(window, cx) {
                                DockPosition::Left => Some(
                                    KeyBinding::for_action(&workspace::ToggleSidebar, cx)
                                        .into_any_element(),
                                ),
                                DockPosition::Bottom => None,
                                DockPosition::Right => Some(
                                    KeyBinding::for_action(&workspace::ToggleProjectPane, cx)
                                        .into_any_element(),
                                ),
                            },
                            |this, key_binding| this.child(key_binding),
                        ),
                )
        } else {
            let list_contents = {
                let items_len = self.cached_entries.len();
                let multi_buffer_snapshot = self
                    .active_editor()
                    .map(|editor| editor.read(cx).buffer().read(cx).snapshot(cx));
                uniform_list(
                    "entries",
                    items_len,
                    cx.processor(move |outline_panel, range: Range<usize>, window, cx| {
                        outline_panel.rendered_entries_len = range.end - range.start;
                        let entries = outline_panel.cached_entries.get(range);
                        entries
                            .map(|entries| entries.to_vec())
                            .unwrap_or_default()
                            .into_iter()
                            .filter_map(|cached_entry| match cached_entry.entry {
                                PanelEntry::Fs(entry) => Some(outline_panel.render_entry(
                                    &entry,
                                    cached_entry.depth,
                                    cached_entry.string_match.as_ref(),
                                    window,
                                    cx,
                                )),
                                PanelEntry::FoldedDirs(folded_dirs_entry) => {
                                    Some(outline_panel.render_folded_dirs(
                                        &folded_dirs_entry,
                                        cached_entry.depth,
                                        cached_entry.string_match.as_ref(),
                                        window,
                                        cx,
                                    ))
                                }
                                PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                                    outline_panel.render_excerpt(
                                        &excerpt,
                                        cached_entry.depth,
                                        window,
                                        cx,
                                    )
                                }
                                PanelEntry::Outline(OutlineEntry::Outline(entry)) => {
                                    Some(outline_panel.render_outline(
                                        &entry,
                                        cached_entry.depth,
                                        cached_entry.string_match.as_ref(),
                                        window,
                                        cx,
                                    ))
                                }
                                PanelEntry::Search(SearchEntry {
                                    match_range,
                                    render_data,
                                    kind,
                                    ..
                                }) => outline_panel.render_search_match(
                                    multi_buffer_snapshot.as_ref(),
                                    &match_range,
                                    &render_data,
                                    kind,
                                    cached_entry.depth,
                                    cached_entry.string_match.as_ref(),
                                    window,
                                    cx,
                                ),
                            })
                            .collect()
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(self.max_width_item_index)
                .track_scroll(&self.scroll_handle)
                .when(show_indent_guides, |list| {
                    list.with_decoration(
                        ui::indent_guides(px(indent_size), IndentGuideColors::panel(cx))
                            .with_compute_indents_fn(cx.entity(), |outline_panel, range, _, _| {
                                let entries = outline_panel.cached_entries.get(range);
                                if let Some(entries) = entries {
                                    entries.iter().map(|item| item.depth).collect()
                                } else {
                                    smallvec::SmallVec::new()
                                }
                            })
                            .with_render_fn(cx.entity(), move |outline_panel, params, _, _| {
                                const LEFT_OFFSET: Pixels = px(14.);

                                let indent_size = params.indent_size;
                                let item_height = params.item_height;
                                let active_indent_guide_ix = find_active_indent_guide_ix(
                                    outline_panel,
                                    &params.indent_guides,
                                );

                                params
                                    .indent_guides
                                    .into_iter()
                                    .enumerate()
                                    .map(|(ix, layout)| {
                                        let bounds = Bounds::new(
                                            point(
                                                layout.offset.x * indent_size + LEFT_OFFSET,
                                                layout.offset.y * item_height,
                                            ),
                                            size(px(1.), layout.length * item_height),
                                        );
                                        ui::RenderedIndentGuide {
                                            bounds,
                                            layout,
                                            is_active: active_indent_guide_ix == Some(ix),
                                            hitbox: None,
                                        }
                                    })
                                    .collect()
                            }),
                    )
                })
            };

            v_flex()
                .flex_shrink_1()
                .size_full()
                .child(list_contents.size_full().flex_shrink_1())
                .custom_scrollbars(
                    Scrollbars::for_settings::<OutlinePanelSettingsScrollbarProxy>()
                        .tracked_scroll_handle(&self.scroll_handle.clone())
                        .with_track_along(
                            ScrollAxes::Horizontal,
                            cx.theme().colors().editor_background,
                        )
                        .tracked_entity(cx.entity_id()),
                    window,
                    cx,
                )
        }
        .children(self.context_menu.as_ref().map(|(menu, position, _)| {
            deferred(
                anchored()
                    .position(*position)
                    .anchor(gpui::Anchor::TopLeft)
                    .child(menu.clone()),
            )
            .with_priority(1)
        }));

        v_flex().w_full().flex_1().overflow_hidden().child(contents)
    }

    fn render_filter_footer(&mut self, pinned: bool, cx: &mut Context<Self>) -> Div {
        let (pin_button_id, icon, icon_tooltip) = if pinned {
            ("unpin_button", IconName::Unpin, "Unpin Outline")
        } else {
            ("pin_button", IconName::Pin, "Pin Active Outline")
        };

        let has_query = self.query(cx).is_some();

        h_flex()
            .p_2()
            .h(Tab::container_height(cx))
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::MagnifyingGlass)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(self.filter_editor.clone()),
            )
            .child(
                h_flex()
                    .when(has_query, |this| {
                        this.child(
                            IconButton::new("clear_filter", IconName::Close)
                                .shape(IconButtonShape::Square)
                                .tooltip(Tooltip::text("Clear Filter"))
                                .on_click(cx.listener(|outline_panel, _, window, cx| {
                                    outline_panel.filter_editor.update(cx, |editor, cx| {
                                        editor.set_text("", window, cx);
                                    });
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        IconButton::new(pin_button_id, icon)
                            .tooltip(Tooltip::text(icon_tooltip))
                            .shape(IconButtonShape::Square)
                            .on_click(cx.listener(|outline_panel, _, window, cx| {
                                outline_panel.toggle_active_editor_pin(
                                    &ToggleActiveEditorPin,
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
    }

    fn buffers_inside_directory(
        &self,
        dir_worktree: WorktreeId,
        dir_entry: &GitEntry,
    ) -> HashSet<BufferId> {
        if !dir_entry.is_dir() {
            debug_panic!("buffers_inside_directory called on a non-directory entry {dir_entry:?}");
            return HashSet::default();
        }

        self.fs_entries
            .iter()
            .skip_while(|fs_entry| match fs_entry {
                FsEntry::Directory(directory) => {
                    directory.worktree_id != dir_worktree || &directory.entry != dir_entry
                }
                _ => true,
            })
            .skip(1)
            .take_while(|fs_entry| match fs_entry {
                FsEntry::ExternalFile(..) => false,
                FsEntry::Directory(directory) => {
                    directory.worktree_id == dir_worktree
                        && directory.entry.path.starts_with(&dir_entry.path)
                }
                FsEntry::File(file) => {
                    file.worktree_id == dir_worktree && file.entry.path.starts_with(&dir_entry.path)
                }
            })
            .filter_map(|fs_entry| match fs_entry {
                FsEntry::File(file) => Some(file.buffer_id),
                _ => None,
            })
            .collect()
    }
}

fn back_to_common_visited_parent(
    visited_dirs: &mut Vec<(ProjectEntryId, Arc<RelPath>)>,
    worktree_id: &WorktreeId,
    new_entry: &Entry,
) -> Option<(WorktreeId, ProjectEntryId)> {
    while let Some((visited_dir_id, visited_path)) = visited_dirs.last() {
        match new_entry.path.parent() {
            Some(parent_path) => {
                if parent_path == visited_path.as_ref() {
                    return Some((*worktree_id, *visited_dir_id));
                }
            }
            None => {
                break;
            }
        }
        visited_dirs.pop();
    }
    None
}

fn file_name(path: &Path) -> String {
    let mut current_path = path;
    loop {
        if let Some(file_name) = current_path.file_name() {
            return file_name.to_string_lossy().into_owned();
        }
        match current_path.parent() {
            Some(parent) => current_path = parent,
            None => return path.to_string_lossy().into_owned(),
        }
    }
}

fn find_active_indent_guide_ix(
    outline_panel: &OutlinePanel,
    candidates: &[IndentGuideLayout],
) -> Option<usize> {
    let SelectedEntry::Valid(_, target_ix) = &outline_panel.selected_entry else {
        return None;
    };
    let target_depth = outline_panel
        .cached_entries
        .get(*target_ix)
        .map(|cached_entry| cached_entry.depth)?;

    let (target_ix, target_depth) = if let Some(target_depth) = outline_panel
        .cached_entries
        .get(target_ix + 1)
        .filter(|cached_entry| cached_entry.depth > target_depth)
        .map(|entry| entry.depth)
    {
        (target_ix + 1, target_depth.saturating_sub(1))
    } else {
        (*target_ix, target_depth.saturating_sub(1))
    };

    candidates
        .iter()
        .enumerate()
        .find(|(_, guide)| {
            guide.offset.y <= target_ix
                && target_ix < guide.offset.y + guide.length
                && guide.offset.x == target_depth
        })
        .map(|(ix, _)| ix)
}

fn subscribe_for_editor_events(
    editor: &Entity<Editor>,
    window: &mut Window,
    cx: &mut Context<OutlinePanel>,
) -> Subscription {
    let debounce = Some(UPDATE_DEBOUNCE);
    cx.subscribe_in(
        editor,
        window,
        move |outline_panel, editor, e: &EditorEvent, window, cx| {
            if !outline_panel.active {
                return;
            }
            match e {
                EditorEvent::SelectionsChanged { local: true } => {
                    outline_panel.reveal_entry_for_selection(editor.clone(), window, cx);
                    cx.notify();
                }
                EditorEvent::BuffersRemoved { removed_buffer_ids } => {
                    outline_panel
                        .buffers
                        .retain(|buffer_id, _| !removed_buffer_ids.contains(buffer_id));
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::BufferRangesUpdated { buffer, .. } => {
                    outline_panel
                        .new_entries_for_fs_update
                        .insert(buffer.read(cx).remote_id());
                    outline_panel.invalidate_outlines(&[buffer.read(cx).remote_id()]);
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::BuffersEdited { buffer_ids } => {
                    outline_panel.invalidate_outlines(buffer_ids);
                    let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                    if update_cached_items {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::BufferFoldToggled { ids, .. } => {
                    outline_panel.invalidate_outlines(ids);
                    let mut latest_unfolded_buffer_id = None;
                    let mut latest_folded_buffer_id = None;
                    let mut ignore_selections_change = false;
                    outline_panel.new_entries_for_fs_update.extend(
                        ids.iter()
                            .filter(|id| {
                                if outline_panel.buffers.contains_key(&id) {
                                    ignore_selections_change |= outline_panel
                                        .preserve_selection_on_buffer_fold_toggles
                                        .remove(&id);
                                    if editor.read(cx).is_buffer_folded(**id, cx) {
                                        latest_folded_buffer_id = Some(**id);
                                        false
                                    } else {
                                        latest_unfolded_buffer_id = Some(**id);
                                        true
                                    }
                                } else {
                                    false
                                }
                            })
                            .copied(),
                    );
                    if !ignore_selections_change
                        && let Some(entry_to_select) = latest_unfolded_buffer_id
                            .or(latest_folded_buffer_id)
                            .and_then(|toggled_buffer_id| {
                                outline_panel.fs_entries.iter().find_map(
                                    |fs_entry| match fs_entry {
                                        FsEntry::ExternalFile(external) => {
                                            if external.buffer_id == toggled_buffer_id {
                                                Some(fs_entry.clone())
                                            } else {
                                                None
                                            }
                                        }
                                        FsEntry::File(FsEntryFile { buffer_id, .. }) => {
                                            if *buffer_id == toggled_buffer_id {
                                                Some(fs_entry.clone())
                                            } else {
                                                None
                                            }
                                        }
                                        FsEntry::Directory(..) => None,
                                    },
                                )
                            })
                            .map(PanelEntry::Fs)
                    {
                        outline_panel.select_entry(entry_to_select, true, window, cx);
                    }

                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                EditorEvent::Reparsed(buffer_id) => {
                    if let Some(buffer) = outline_panel.buffers.get_mut(buffer_id) {
                        buffer.invalidate_outlines();
                    }
                    let update_cached_items = outline_panel.update_non_fs_items(window, cx);
                    if update_cached_items {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::OutlineSymbolsChanged => {
                    for buffer in outline_panel.buffers.values_mut() {
                        buffer.invalidate_outlines();
                    }
                    if matches!(
                        outline_panel.selected_entry(),
                        Some(PanelEntry::Outline(..)),
                    ) {
                        outline_panel.selected_entry.invalidate();
                    }
                    if outline_panel.update_non_fs_items(window, cx) {
                        outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                    }
                }
                EditorEvent::TitleChanged => {
                    outline_panel.update_fs_entries(editor.clone(), debounce, window, cx);
                }
                _ => {}
            }
        },
    )
}

fn empty_icon() -> AnyElement {
    h_flex()
        .size(IconSize::default().rems())
        .invisible()
        .flex_none()
        .into_any_element()
}

#[derive(Debug, Default)]
struct GenerationState {
    entries: Vec<CachedEntry>,
    match_candidates: Vec<StringMatchCandidate>,
    max_width_estimate_and_index: Option<(u64, usize)>,
}

impl GenerationState {
    fn clear(&mut self) {
        self.entries.clear();
        self.match_candidates.clear();
        self.max_width_estimate_and_index = None;
    }
}

#[cfg(test)]
mod tests {
    use db::indoc;
    use futures::stream::StreamExt as _;
    use gpui::{TestAppContext, UpdateGlobal, VisualTestContext, WindowHandle};
    use language::{self, FakeLspAdapter, markdown_lang, rust_lang};
    use pretty_assertions::assert_eq;
    use project::FakeFs;
    use search::{
        buffer_search,
        project_search::{self, perform_project_search},
    };
    use serde_json::json;
    use util::path;
    use workspace::{MultiWorkspace, OpenOptions, OpenVisible, ToolbarItemView};

    use super::*;

    const SELECTED_MARKER: &str = "  <==== selected";

    #[gpui::test(iterations = 10)]
    async fn test_project_search_results_toggling(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        let root = path!("/rust-analyzer");
        populate_with_test_ra_project(&fs, root).await;
        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace.update_in(cx, |workspace, window, cx| {
            ProjectSearchView::deploy_search(
                workspace,
                &workspace::DeploySearch::default(),
                window,
                cx,
            )
        });
        let search_view = workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .active_pane()
                .read(cx)
                .items()
                .find_map(|item| item.downcast::<ProjectSearchView>())
                .expect("Project search view expected to appear after new search event trigger")
        });

        let query = "param_names_for_lifetime_elision_hints";
        perform_project_search(&search_view, query, cx);
        search_view.update(cx, |search_view, cx| {
            search_view
                .results_editor()
                .update(cx, |results_editor, cx| {
                    assert_eq!(
                        results_editor.display_text(cx).match_indices(query).count(),
                        9
                    );
                });
        });

        let all_matches = r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs
          search: match config.«param_names_for_lifetime_elision_hints» {
          search: allocated_lifetimes.push(if config.«param_names_for_lifetime_elision_hints» {
          search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {
          search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },
      inlay_hints.rs
        search: pub «param_names_for_lifetime_elision_hints»: bool,
        search: «param_names_for_lifetime_elision_hints»: self
      static_index.rs
        search: «param_names_for_lifetime_elision_hints»: false,
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#
            .to_string();

        let select_first_in_all_matches = |line_to_select: &str| {
            assert!(
                all_matches.contains(line_to_select),
                "`{line_to_select}` was not found in all matches `{all_matches}`"
            );
            all_matches.replacen(
                line_to_select,
                &format!("{line_to_select}{SELECTED_MARKER}"),
                1,
            )
        };

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches(
                    "search: match config.«param_names_for_lifetime_elision_hints» {"
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_parent(&SelectParent, window, cx);
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches("fn_lifetime_fn.rs")
            );
        });
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs{SELECTED_MARKER}
      inlay_hints.rs
        search: pub «param_names_for_lifetime_elision_hints»: bool,
        search: «param_names_for_lifetime_elision_hints»: self
      static_index.rs
        search: «param_names_for_lifetime_elision_hints»: false,
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#,
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.expand_all_entries(&ExpandAllEntries, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_parent(&SelectParent, window, cx);
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches("inlay_hints/")
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_parent(&SelectParent, window, cx);
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches("ide/src/")
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"rust-analyzer/
  crates/
    ide/src/{SELECTED_MARKER}
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#,
                )
            );
        });
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches("ide/src/")
            );
        });
    }

    #[gpui::test(iterations = 10)]
    async fn test_item_filtering(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        let root = path!("/rust-analyzer");
        populate_with_test_ra_project(&fs, root).await;
        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace.update_in(cx, |workspace, window, cx| {
            ProjectSearchView::deploy_search(
                workspace,
                &workspace::DeploySearch::default(),
                window,
                cx,
            )
        });
        let search_view = workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .active_pane()
                .read(cx)
                .items()
                .find_map(|item| item.downcast::<ProjectSearchView>())
                .expect("Project search view expected to appear after new search event trigger")
        });

        let query = "param_names_for_lifetime_elision_hints";
        perform_project_search(&search_view, query, cx);
        search_view.update(cx, |search_view, cx| {
            search_view
                .results_editor()
                .update(cx, |results_editor, cx| {
                    assert_eq!(
                        results_editor.display_text(cx).match_indices(query).count(),
                        9
                    );
                });
        });
        let all_matches = r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs
          search: match config.«param_names_for_lifetime_elision_hints» {
          search: allocated_lifetimes.push(if config.«param_names_for_lifetime_elision_hints» {
          search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {
          search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },
      inlay_hints.rs
        search: pub «param_names_for_lifetime_elision_hints»: bool,
        search: «param_names_for_lifetime_elision_hints»: self
      static_index.rs
        search: «param_names_for_lifetime_elision_hints»: false,
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#
            .to_string();

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    None,
                    cx,
                ),
                all_matches,
            );
        });

        let filter_text = "a";
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.filter_editor.update(cx, |filter_editor, cx| {
                filter_editor.set_text(filter_text, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    None,
                    cx,
                ),
                all_matches
                    .lines()
                    .skip(1) // `/rust-analyzer/` is a root entry with path `` and it will be filtered out
                    .filter(|item| item.contains(filter_text))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.filter_editor.update(cx, |filter_editor, cx| {
                filter_editor.set_text("", window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    None,
                    cx,
                ),
                all_matches,
            );
        });
    }

    #[gpui::test(iterations = 10)]
    async fn test_item_opening(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        let root = path!("/rust-analyzer");
        populate_with_test_ra_project(&fs, root).await;
        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace.update_in(cx, |workspace, window, cx| {
            ProjectSearchView::deploy_search(
                workspace,
                &workspace::DeploySearch::default(),
                window,
                cx,
            )
        });
        let search_view = workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .active_pane()
                .read(cx)
                .items()
                .find_map(|item| item.downcast::<ProjectSearchView>())
                .expect("Project search view expected to appear after new search event trigger")
        });

        let query = "param_names_for_lifetime_elision_hints";
        perform_project_search(&search_view, query, cx);
        search_view.update(cx, |search_view, cx| {
            search_view
                .results_editor()
                .update(cx, |results_editor, cx| {
                    assert_eq!(
                        results_editor.display_text(cx).match_indices(query).count(),
                        9
                    );
                });
        });
        let all_matches = r#"rust-analyzer/
  crates/
    ide/src/
      inlay_hints/
        fn_lifetime_fn.rs
          search: match config.«param_names_for_lifetime_elision_hints» {
          search: allocated_lifetimes.push(if config.«param_names_for_lifetime_elision_hints» {
          search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {
          search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },
      inlay_hints.rs
        search: pub «param_names_for_lifetime_elision_hints»: bool,
        search: «param_names_for_lifetime_elision_hints»: self
      static_index.rs
        search: «param_names_for_lifetime_elision_hints»: false,
    rust-analyzer/src/
      cli/
        analysis_stats.rs
          search: «param_names_for_lifetime_elision_hints»: true,
      config.rs
        search: «param_names_for_lifetime_elision_hints»: self"#
            .to_string();
        let select_first_in_all_matches = |line_to_select: &str| {
            assert!(
                all_matches.contains(line_to_select),
                "`{line_to_select}` was not found in all matches `{all_matches}`"
            );
            all_matches.replacen(
                line_to_select,
                &format!("{line_to_select}{SELECTED_MARKER}"),
                1,
            )
        };
        let clear_outline_metadata = |input: &str| {
            input
                .replace("search: ", "")
                .replace("«", "")
                .replace("»", "")
        };

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        let active_editor = outline_panel.read_with(cx, |outline_panel, _| {
            outline_panel
                .active_editor()
                .expect("should have an active editor open")
        });
        let initial_outline_selection =
            "search: match config.«param_names_for_lifetime_elision_hints» {";
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches(initial_outline_selection)
            );
            assert_eq!(
                selected_row_text(&active_editor, cx),
                clear_outline_metadata(initial_outline_selection),
                "Should place the initial editor selection on the corresponding search result"
            );

            outline_panel.select_next(&SelectNext, window, cx);
            outline_panel.select_next(&SelectNext, window, cx);
        });

        let navigated_outline_selection =
            "search: Some(it) if config.«param_names_for_lifetime_elision_hints» => {";
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches(navigated_outline_selection)
            );
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        outline_panel.update(cx, |_, cx| {
            assert_eq!(
                selected_row_text(&active_editor, cx),
                clear_outline_metadata(navigated_outline_selection),
                "Should still have the initial caret position after SelectNext calls"
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.open_selected_entry(&OpenSelectedEntry, window, cx);
        });
        outline_panel.update(cx, |_outline_panel, cx| {
            assert_eq!(
                selected_row_text(&active_editor, cx),
                clear_outline_metadata(navigated_outline_selection),
                "After opening, should move the caret to the opened outline entry's position"
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
        });
        let next_navigated_outline_selection = "search: InlayHintsConfig { «param_names_for_lifetime_elision_hints»: true, ..TEST_CONFIG },";
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                select_first_in_all_matches(next_navigated_outline_selection)
            );
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        outline_panel.update(cx, |_outline_panel, cx| {
            assert_eq!(
                selected_row_text(&active_editor, cx),
                clear_outline_metadata(next_navigated_outline_selection),
                "Should again preserve the selection after another SelectNext call"
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.open_excerpts(&editor::actions::OpenExcerpts, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        let new_active_editor = outline_panel.read_with(cx, |outline_panel, _| {
            outline_panel
                .active_editor()
                .expect("should have an active editor open")
        });
        outline_panel.update(cx, |outline_panel, cx| {
            assert_ne!(
                active_editor, new_active_editor,
                "After opening an excerpt, new editor should be open"
            );
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                "outline: pub(super) fn hints
outline: fn hints_lifetimes_named  <==== selected"
            );
            assert_eq!(
                selected_row_text(&new_active_editor, cx),
                clear_outline_metadata(next_navigated_outline_selection),
                "When opening the excerpt, should navigate to the place corresponding the outline entry"
            );
        });
    }

    #[gpui::test]
    async fn test_multiple_worktrees(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/root"),
            json!({
                "one": {
                    "a.txt": "aaa aaa"
                },
                "two": {
                    "b.txt": "a aaa"
                }

            }),
        )
        .await;
        let project = Project::test(fs.clone(), [Path::new(path!("/root/one"))], cx).await;
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        let items = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_paths(
                    vec![PathBuf::from(path!("/root/two"))],
                    OpenOptions {
                        visible: Some(OpenVisible::OnlyDirectories),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
            .await;
        assert_eq!(items.len(), 1, "Were opening another worktree directory");
        assert!(
            items[0].is_none(),
            "Directory should be opened successfully"
        );

        workspace.update_in(cx, |workspace, window, cx| {
            ProjectSearchView::deploy_search(
                workspace,
                &workspace::DeploySearch::default(),
                window,
                cx,
            )
        });
        let search_view = workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .active_pane()
                .read(cx)
                .items()
                .find_map(|item| item.downcast::<ProjectSearchView>())
                .expect("Project search view expected to appear after new search event trigger")
        });

        let query = "aaa";
        perform_project_search(&search_view, query, cx);
        search_view.update(cx, |search_view, cx| {
            search_view
                .results_editor()
                .update(cx, |results_editor, cx| {
                    assert_eq!(
                        results_editor.display_text(cx).match_indices(query).count(),
                        3
                    );
                });
        });

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"one/
  a.txt
    search: «aaa» aaa  <==== selected
    search: aaa «aaa»
two/
  b.txt
    search: a «aaa»"#,
                ),
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"one/
  a.txt  <==== selected
two/
  b.txt
    search: a «aaa»"#,
                ),
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"one/
  a.txt
two/  <==== selected"#,
                ),
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"one/
  a.txt
two/  <==== selected
  b.txt
    search: a «aaa»"#,
                )
            );
        });
    }

    #[gpui::test]
    async fn test_navigating_in_singleton(cx: &mut TestAppContext) {
        init_test(cx);

        let root = path!("/root");
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            root,
            json!({
                "src": {
                    "lib.rs": indoc!("
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OutlineEntryExcerpt {
    id: ExcerptId,
    buffer_id: BufferId,
    range: ExcerptRange<language::Anchor>,
}"),
                }
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.set_active(true, window, cx)
            });
        });

        let _editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from(path!("/root/src/lib.rs")),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .expect("Failed to open Rust source file")
            .downcast::<Editor>()
            .expect("Should open an editor for Rust source file");

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_next(&SelectNext, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_next(&SelectNext, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id  <==== selected
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_next(&SelectNext, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id  <==== selected
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_next(&SelectNext, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_next(&SelectNext, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_previous(&SelectPrevious, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_previous(&SelectPrevious, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id  <==== selected
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_previous(&SelectPrevious, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id  <==== selected
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_previous(&SelectPrevious, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_previous(&SelectPrevious, window, cx);
            });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
                )
            );
        });
    }

    #[gpui::test(iterations = 10)]
    async fn test_frontend_repo_structure(cx: &mut TestAppContext) {
        init_test(cx);

        let root = path!("/frontend-project");
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            root,
            json!({
                "public": {
                    "lottie": {
                        "syntax-tree.json": r#"{ "something": "static" }"#
                    }
                },
                "src": {
                    "app": {
                        "(site)": {
                            "(about)": {
                                "jobs": {
                                    "[slug]": {
                                        "page.tsx": r#"static"#
                                    }
                                }
                            },
                            "(blog)": {
                                "post": {
                                    "[slug]": {
                                        "page.tsx": r#"static"#
                                    }
                                }
                            },
                        }
                    },
                    "components": {
                        "ErrorBoundary.tsx": r#"static"#,
                    }
                }

            }),
        )
        .await;
        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace.update_in(cx, |workspace, window, cx| {
            ProjectSearchView::deploy_search(
                workspace,
                &workspace::DeploySearch::default(),
                window,
                cx,
            )
        });
        let search_view = workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .active_pane()
                .read(cx)
                .items()
                .find_map(|item| item.downcast::<ProjectSearchView>())
                .expect("Project search view expected to appear after new search event trigger")
        });

        let query = "static";
        perform_project_search(&search_view, query, cx);
        search_view.update(cx, |search_view, cx| {
            search_view
                .results_editor()
                .update(cx, |results_editor, cx| {
                    assert_eq!(
                        results_editor.display_text(cx).match_indices(query).count(),
                        4
                    );
                });
        });

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}  <==== selected
  src/
    app/(site)/
      (about)/jobs/[slug]/
        page.tsx
          search: «static»
      (blog)/post/[slug]/
        page.tsx
          search: «static»
    components/
      ErrorBoundary.tsx
        search: «static»"#
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            // Move to 5th element in the list, 3 items down.
            for _ in 0..2 {
                outline_panel.select_next(&SelectNext, window, cx);
            }
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}
  src/
    app/(site)/  <==== selected
    components/
      ErrorBoundary.tsx
        search: «static»"#
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            // Move to the next visible non-FS entry
            for _ in 0..3 {
                outline_panel.select_next(&SelectNext, window, cx);
            }
        });
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}
  src/
    app/(site)/
    components/
      ErrorBoundary.tsx
        search: «static»  <==== selected"#
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel
                .active_editor()
                .expect("Should have an active editor")
                .update(cx, |editor, cx| {
                    editor.toggle_fold(&editor::actions::ToggleFold, window, cx)
                });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}
  src/
    app/(site)/
    components/
      ErrorBoundary.tsx  <==== selected"#
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel
                .active_editor()
                .expect("Should have an active editor")
                .update(cx, |editor, cx| {
                    editor.toggle_fold(&editor::actions::ToggleFold, window, cx)
                });
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}
  src/
    app/(site)/
    components/
      ErrorBoundary.tsx  <==== selected
        search: «static»"#
                )
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.collapse_all_entries(&CollapseAllEntries, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(r#"frontend-project/"#)
            );
        });

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.expand_all_entries(&ExpandAllEntries, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                format!(
                    r#"frontend-project/
  public/lottie/
    syntax-tree.json
      search: {{ "something": "«static»" }}
  src/
    app/(site)/
      (about)/jobs/[slug]/
        page.tsx
          search: «static»
      (blog)/post/[slug]/
        page.tsx
          search: «static»
    components/
      ErrorBoundary.tsx  <==== selected
        search: «static»"#
                )
            );
        });
    }

    async fn add_outline_panel(
        project: &Entity<Project>,
        cx: &mut TestAppContext,
    ) -> (WindowHandle<MultiWorkspace>, Entity<Workspace>) {
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();

        let workspace_weak = workspace.downgrade();
        let outline_panel = window
            .update(cx, |_, window, cx| {
                cx.spawn_in(window, async move |_this, cx| {
                    OutlinePanel::load(workspace_weak, cx.clone()).await
                })
            })
            .unwrap()
            .await
            .expect("Failed to load outline panel");

        window
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    workspace.add_panel(outline_panel, window, cx);
                });
            })
            .unwrap();
        (window, workspace)
    }

    fn outline_panel(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> Entity<OutlinePanel> {
        workspace.update_in(cx, |workspace, _window, cx| {
            workspace
                .panel::<OutlinePanel>(cx)
                .expect("no outline panel")
        })
    }

    fn display_entries(
        project: &Entity<Project>,
        multi_buffer_snapshot: &MultiBufferSnapshot,
        cached_entries: &[CachedEntry],
        selected_entry: Option<&PanelEntry>,
        cx: &mut App,
    ) -> String {
        let project = project.read(cx);
        let mut display_string = String::new();
        for entry in cached_entries {
            if !display_string.is_empty() {
                display_string += "\n";
            }
            for _ in 0..entry.depth {
                display_string += "  ";
            }
            display_string += &match &entry.entry {
                PanelEntry::Fs(entry) => match entry {
                    FsEntry::ExternalFile(_) => {
                        panic!("Did not cover external files with tests")
                    }
                    FsEntry::Directory(directory) => {
                        let path = if let Some(worktree) = project
                            .worktree_for_id(directory.worktree_id, cx)
                            .filter(|worktree| {
                                worktree.read(cx).root_entry() == Some(&directory.entry.entry)
                            }) {
                            worktree
                                .read(cx)
                                .root_name()
                                .join(&directory.entry.path)
                                .as_unix_str()
                                .to_string()
                        } else {
                            directory
                                .entry
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string()
                        };
                        format!("{path}/")
                    }
                    FsEntry::File(file) => file
                        .entry
                        .path
                        .file_name()
                        .map(|name| name.to_string())
                        .unwrap_or_default(),
                },
                PanelEntry::FoldedDirs(folded_dirs) => folded_dirs
                    .entries
                    .iter()
                    .filter_map(|dir| dir.path.file_name())
                    .map(|name| name.to_string() + "/")
                    .collect(),
                PanelEntry::Outline(outline_entry) => match outline_entry {
                    OutlineEntry::Excerpt(_) => continue,
                    OutlineEntry::Outline(outline_entry) => {
                        format!("outline: {}", outline_entry.text)
                    }
                },
                PanelEntry::Search(search_entry) => {
                    let search_data = search_entry.render_data.get_or_init(|| {
                        SearchData::new(&search_entry.match_range, multi_buffer_snapshot)
                    });
                    let mut search_result = String::new();
                    let mut last_end = 0;
                    for range in &search_data.search_match_indices {
                        search_result.push_str(&search_data.context_text[last_end..range.start]);
                        search_result.push('«');
                        search_result.push_str(&search_data.context_text[range.start..range.end]);
                        search_result.push('»');
                        last_end = range.end;
                    }
                    search_result.push_str(&search_data.context_text[last_end..]);

                    format!("search: {search_result}")
                }
            };

            if Some(&entry.entry) == selected_entry {
                display_string += SELECTED_MARKER;
            }
        }
        display_string
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings = SettingsStore::test(cx);
            cx.set_global(settings);

            theme_settings::init(theme::LoadThemes::JustBase, cx);

            editor::init(cx);
            project_search::init(cx);
            buffer_search::init(cx);
            super::init(cx);
        });
    }

    // Based on https://github.com/rust-lang/rust-analyzer/
    async fn populate_with_test_ra_project(fs: &FakeFs, root: &str) {
        fs.insert_tree(
            root,
            json!({
                    "crates": {
                        "ide": {
                            "src": {
                                "inlay_hints": {
                                    "fn_lifetime_fn.rs": r##"
        pub(super) fn hints(
            acc: &mut Vec<InlayHint>,
            config: &InlayHintsConfig,
            func: ast::Fn,
        ) -> Option<()> {
            // ... snip

            let mut used_names: FxHashMap<SmolStr, usize> =
                match config.param_names_for_lifetime_elision_hints {
                    true => generic_param_list
                        .iter()
                        .flat_map(|gpl| gpl.lifetime_params())
                        .filter_map(|param| param.lifetime())
                        .filter_map(|lt| Some((SmolStr::from(lt.text().as_str().get(1..)?), 0)))
                        .collect(),
                    false => Default::default(),
                };
            {
                let mut potential_lt_refs = potential_lt_refs.iter().filter(|&&(.., is_elided)| is_elided);
                if self_param.is_some() && potential_lt_refs.next().is_some() {
                    allocated_lifetimes.push(if config.param_names_for_lifetime_elision_hints {
                        // self can't be used as a lifetime, so no need to check for collisions
                        "'self".into()
                    } else {
                        gen_idx_name()
                    });
                }
                potential_lt_refs.for_each(|(name, ..)| {
                    let name = match name {
                        Some(it) if config.param_names_for_lifetime_elision_hints => {
                            if let Some(c) = used_names.get_mut(it.text().as_str()) {
                                *c += 1;
                                SmolStr::from(format!("'{text}{c}", text = it.text().as_str()))
                            } else {
                                used_names.insert(it.text().as_str().into(), 0);
                                SmolStr::from_iter(["\'", it.text().as_str()])
                            }
                        }
                        _ => gen_idx_name(),
                    };
                    allocated_lifetimes.push(name);
                });
            }

            // ... snip
        }

        // ... snip

            #[test]
            fn hints_lifetimes_named() {
                check_with_config(
                    InlayHintsConfig { param_names_for_lifetime_elision_hints: true, ..TEST_CONFIG },
                    r#"
        fn nested_in<'named>(named: &        &X<      &()>) {}
        //          ^'named1, 'named2, 'named3, $
                                  //^'named1 ^'named2 ^'named3
        "#,
                );
            }

        // ... snip
        "##,
                                },
                        "inlay_hints.rs": r#"
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct InlayHintsConfig {
        // ... snip
        pub param_names_for_lifetime_elision_hints: bool,
        pub max_length: Option<usize>,
        // ... snip
    }

    impl Config {
        pub fn inlay_hints(&self) -> InlayHintsConfig {
            InlayHintsConfig {
                // ... snip
                param_names_for_lifetime_elision_hints: self
                    .inlayHints_lifetimeElisionHints_useParameterNames()
                    .to_owned(),
                max_length: self.inlayHints_maxLength().to_owned(),
                // ... snip
            }
        }
    }
    "#,
                        "static_index.rs": r#"
// ... snip
        fn add_file(&mut self, file_id: FileId) {
            let current_crate = crates_for(self.db, file_id).pop().map(Into::into);
            let folds = self.analysis.folding_ranges(file_id).unwrap();
            let inlay_hints = self
                .analysis
                .inlay_hints(
                    &InlayHintsConfig {
                        // ... snip
                        closure_style: hir::ClosureStyle::ImplFn,
                        param_names_for_lifetime_elision_hints: false,
                        binding_mode_hints: false,
                        max_length: Some(25),
                        closure_capture_hints: false,
                        // ... snip
                    },
                    file_id,
                    None,
                )
                .unwrap();
            // ... snip
    }
// ... snip
    "#
                            }
                        },
                        "rust-analyzer": {
                            "src": {
                                "cli": {
                                    "analysis_stats.rs": r#"
        // ... snip
                for &file_id in &file_ids {
                    _ = analysis.inlay_hints(
                        &InlayHintsConfig {
                            // ... snip
                            implicit_drop_hints: true,
                            lifetime_elision_hints: ide::LifetimeElisionHints::Always,
                            param_names_for_lifetime_elision_hints: true,
                            hide_named_constructor_hints: false,
                            hide_closure_initialization_hints: false,
                            closure_style: hir::ClosureStyle::ImplFn,
                            max_length: Some(25),
                            closing_brace_hints_min_lines: Some(20),
                            fields_to_resolve: InlayFieldsToResolve::empty(),
                            range_exclusive_hints: true,
                        },
                        file_id.into(),
                        None,
                    );
                }
        // ... snip
                                    "#,
                                },
                                "config.rs": r#"
                config_data! {
                    /// Configs that only make sense when they are set by a client. As such they can only be defined
                    /// by setting them using client's settings (e.g `settings.json` on VS Code).
                    client: struct ClientDefaultConfigData <- ClientConfigInput -> {
                        // ... snip
                        /// Maximum length for inlay hints. Set to null to have an unlimited length.
                        inlayHints_maxLength: Option<usize>                        = Some(25),
                        // ... snip
                        /// Whether to prefer using parameter names as the name for elided lifetime hints if possible.
                        inlayHints_lifetimeElisionHints_useParameterNames: bool    = false,
                        // ... snip
                    }
                }

                impl Config {
                    // ... snip
                    pub fn inlay_hints(&self) -> InlayHintsConfig {
                        InlayHintsConfig {
                            // ... snip
                            param_names_for_lifetime_elision_hints: self
                                .inlayHints_lifetimeElisionHints_useParameterNames()
                                .to_owned(),
                            max_length: self.inlayHints_maxLength().to_owned(),
                            // ... snip
                        }
                    }
                    // ... snip
                }
                "#
                                }
                        }
                    }
            }),
        )
        .await;
    }

    fn snapshot(outline_panel: &OutlinePanel, cx: &App) -> MultiBufferSnapshot {
        outline_panel
            .active_editor()
            .unwrap()
            .read(cx)
            .buffer()
            .read(cx)
            .snapshot(cx)
    }

    fn selected_row_text(editor: &Entity<Editor>, cx: &mut App) -> String {
        editor.update(cx, |editor, cx| {
            let selections = editor.selections.all::<language::Point>(&editor.display_snapshot(cx));
            assert_eq!(selections.len(), 1, "Active editor should have exactly one selection after any outline panel interactions");
            let selection = selections.first().unwrap();
            let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            let line_start = language::Point::new(selection.start.row, 0);
            let line_end = multi_buffer_snapshot.clip_point(language::Point::new(selection.end.row, u32::MAX), language::Bias::Right);
            multi_buffer_snapshot.text_for_range(line_start..line_end).collect::<String>().trim().to_owned()
        })
    }

    #[gpui::test]
    async fn test_outline_keyboard_expand_collapse(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/test",
            json!({
                "src": {
                    "lib.rs": indoc!("
                            mod outer {
                                pub struct OuterStruct {
                                    field: String,
                                }
                                impl OuterStruct {
                                    pub fn new() -> Self {
                                        Self { field: String::new() }
                                    }
                                    pub fn method(&self) {
                                        println!(\"{}\", self.field);
                                    }
                                }
                                mod inner {
                                    pub fn inner_function() {
                                        let x = 42;
                                        println!(\"{}\", x);
                                    }
                                    pub struct InnerStruct {
                                        value: i32,
                                    }
                                }
                            }
                            fn main() {
                                let s = outer::OuterStruct::new();
                                s.method();
                            }
                        "),
                }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from("/test/src/lib.rs"),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .unwrap();

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
        cx.run_until_parked();

        // Force another update cycle to ensure outlines are fetched
        outline_panel.update_in(cx, |panel, window, cx| {
            panel.update_non_fs_items(window, cx);
            panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: mod outer  <==== selected
  outline: pub struct OuterStruct
    outline: field
  outline: impl OuterStruct
    outline: pub fn new
    outline: pub fn method
  outline: mod inner
    outline: pub fn inner_function
    outline: pub struct InnerStruct
      outline: value
outline: fn main"
                )
            );
        });

        let parent_outline = outline_panel
            .read_with(cx, |panel, _cx| {
                panel
                    .cached_entries
                    .iter()
                    .find_map(|entry| match &entry.entry {
                        PanelEntry::Outline(OutlineEntry::Outline(outline))
                            if panel
                                .outline_children_cache
                                .get(&outline.range.start.buffer_id)
                                .and_then(|children_map| {
                                    let key = (outline.range.clone(), outline.depth);
                                    children_map.get(&key)
                                })
                                .copied()
                                .unwrap_or(false) =>
                        {
                            Some(entry.entry.clone())
                        }
                        _ => None,
                    })
            })
            .expect("Should find an outline with children");

        outline_panel.update_in(cx, |panel, window, cx| {
            panel.select_entry(parent_outline.clone(), true, window, cx);
            panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: mod outer  <==== selected
outline: fn main"
                )
            );
        });

        outline_panel.update_in(cx, |panel, window, cx| {
            panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: mod outer  <==== selected
  outline: pub struct OuterStruct
    outline: field
  outline: impl OuterStruct
    outline: pub fn new
    outline: pub fn method
  outline: mod inner
    outline: pub fn inner_function
    outline: pub struct InnerStruct
      outline: value
outline: fn main"
                )
            );
        });

        outline_panel.update_in(cx, |panel, window, cx| {
            panel.collapsed_entries.clear();
            panel.update_cached_entries(None, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update_in(cx, |panel, window, cx| {
            let outlines_with_children: Vec<_> = panel
                .cached_entries
                .iter()
                .filter_map(|entry| match &entry.entry {
                    PanelEntry::Outline(OutlineEntry::Outline(outline))
                        if panel
                            .outline_children_cache
                            .get(&outline.range.start.buffer_id)
                            .and_then(|children_map| {
                                let key = (outline.range.clone(), outline.depth);
                                children_map.get(&key)
                            })
                            .copied()
                            .unwrap_or(false) =>
                    {
                        Some(entry.entry.clone())
                    }
                    _ => None,
                })
                .collect();

            for outline in outlines_with_children {
                panel.select_entry(outline, false, window, cx);
                panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
            }
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: mod outer
outline: fn main"
                )
            );
        });

        let collapsed_entries_count =
            outline_panel.read_with(cx, |panel, _| panel.collapsed_entries.len());
        assert!(
            collapsed_entries_count > 0,
            "Should have collapsed entries tracked"
        );
    }

    #[gpui::test]
    async fn test_outline_click_toggle_behavior(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/test",
            json!({
                "src": {
                    "main.rs": indoc!("
                            struct Config {
                                name: String,
                                value: i32,
                            }
                            impl Config {
                                fn new(name: String) -> Self {
                                    Self { name, value: 0 }
                                }
                                fn get_value(&self) -> i32 {
                                    self.value
                                }
                            }
                            enum Status {
                                Active,
                                Inactive,
                            }
                            fn process_config(config: Config) -> Status {
                                if config.get_value() > 0 {
                                    Status::Active
                                } else {
                                    Status::Inactive
                                }
                            }
                            fn main() {
                                let config = Config::new(\"test\".to_string());
                                let status = process_config(config);
                            }
                        "),
                }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));

        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        let _editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from("/test/src/main.rs"),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .unwrap();

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, _cx| {
            outline_panel.selected_entry = SelectedEntry::None;
        });

        // Check initial state - all entries should be expanded by default
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Config
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
                )
            );
        });

        outline_panel.update(cx, |outline_panel, _cx| {
            outline_panel.selected_entry = SelectedEntry::None;
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.select_first(&SelectFirst, window, cx);
            });
        });

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Config  <==== selected
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
            });
        });

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Config  <==== selected
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
                )
            );
        });

        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
            });
        });

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Config  <==== selected
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
                )
            );
        });
    }

    #[gpui::test]
    async fn test_outline_expand_collapse_all(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/test",
            json!({
                "src": {
                    "lib.rs": indoc!("
                            mod outer {
                                pub struct OuterStruct {
                                    field: String,
                                }
                                impl OuterStruct {
                                    pub fn new() -> Self {
                                        Self { field: String::new() }
                                    }
                                    pub fn method(&self) {
                                        println!(\"{}\", self.field);
                                    }
                                }
                                mod inner {
                                    pub fn inner_function() {
                                        let x = 42;
                                        println!(\"{}\", x);
                                    }
                                    pub struct InnerStruct {
                                        value: i32,
                                    }
                                }
                            }
                            fn main() {
                                let s = outer::OuterStruct::new();
                                s.method();
                            }
                        "),
                }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
        project.read_with(cx, |project, _| project.languages().add(rust_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from("/test/src/lib.rs"),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .unwrap();

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
        cx.run_until_parked();

        // Force another update cycle to ensure outlines are fetched
        outline_panel.update_in(cx, |panel, window, cx| {
            panel.update_non_fs_items(window, cx);
            panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: mod outer  <==== selected
  outline: pub struct OuterStruct
    outline: field
  outline: impl OuterStruct
    outline: pub fn new
    outline: pub fn method
  outline: mod inner
    outline: pub fn inner_function
    outline: pub struct InnerStruct
      outline: value
outline: fn main"
                )
            );
        });

        let _parent_outline = outline_panel
            .read_with(cx, |panel, _cx| {
                panel
                    .cached_entries
                    .iter()
                    .find_map(|entry| match &entry.entry {
                        PanelEntry::Outline(OutlineEntry::Outline(outline))
                            if panel
                                .outline_children_cache
                                .get(&outline.range.start.buffer_id)
                                .and_then(|children_map| {
                                    let key = (outline.range.clone(), outline.depth);
                                    children_map.get(&key)
                                })
                                .copied()
                                .unwrap_or(false) =>
                        {
                            Some(entry.entry.clone())
                        }
                        _ => None,
                    })
            })
            .expect("Should find an outline with children");

        // Collapse all entries
        outline_panel.update_in(cx, |panel, window, cx| {
            panel.collapse_all_entries(&CollapseAllEntries, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        let expected_collapsed_output = indoc!(
            "
        outline: mod outer  <==== selected
        outline: fn main"
        );

        outline_panel.update(cx, |panel, cx| {
            assert_eq! {
                display_entries(
                    &project,
                    &snapshot(panel, cx),
                    &panel.cached_entries,
                    panel.selected_entry(),
                    cx,
                ),
                expected_collapsed_output
            };
        });

        // Expand all entries
        outline_panel.update_in(cx, |panel, window, cx| {
            panel.expand_all_entries(&ExpandAllEntries, window, cx);
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        let expected_expanded_output = indoc!(
            "
        outline: mod outer  <==== selected
          outline: pub struct OuterStruct
            outline: field
          outline: impl OuterStruct
            outline: pub fn new
            outline: pub fn method
          outline: mod inner
            outline: pub fn inner_function
            outline: pub struct InnerStruct
              outline: value
        outline: fn main"
        );

        outline_panel.update(cx, |panel, cx| {
            assert_eq! {
                display_entries(
                    &project,
                    &snapshot(panel, cx),
                    &panel.cached_entries,
                    panel.selected_entry(),
                    cx,
                ),
                expected_expanded_output
            };
        });
    }

    #[gpui::test]
    async fn test_buffer_search(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/test",
            json!({
                "foo.txt": r#"<_constitution>

</_constitution>



## 📊 Output

| Field          | Meaning                |
"#
            }),
        )
        .await;

        let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        let editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from("/test/foo.txt"),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..OpenOptions::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .unwrap()
            .downcast::<Editor>()
            .unwrap();

        let search_bar = workspace.update_in(cx, |_, window, cx| {
            cx.new(|cx| {
                let mut search_bar = BufferSearchBar::new(None, window, cx);
                search_bar.set_active_pane_item(Some(&editor), window, cx);
                search_bar.show(window, cx);
                search_bar
            })
        });

        let outline_panel = outline_panel(&workspace, cx);

        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        search_bar
            .update_in(cx, |search_bar, window, cx| {
                search_bar.search("  ", None, true, window, cx)
            })
            .await
            .unwrap();

        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                "search: | Field«  »        | Meaning                |  <==== selected
search: | Field  «  »      | Meaning                |
search: | Field    «  »    | Meaning                |
search: | Field      «  »  | Meaning                |
search: | Field        «  »| Meaning                |
search: | Field          | Meaning«  »              |
search: | Field          | Meaning  «  »            |
search: | Field          | Meaning    «  »          |
search: | Field          | Meaning      «  »        |
search: | Field          | Meaning        «  »      |
search: | Field          | Meaning          «  »    |
search: | Field          | Meaning            «  »  |
search: | Field          | Meaning              «  »|"
            );
        });
    }

    #[gpui::test]
    async fn test_outline_panel_lsp_document_symbols(cx: &mut TestAppContext) {
        init_test(cx);

        let root = path!("/root");
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            root,
            json!({
                "src": {
                    "lib.rs": "struct Foo {\n    bar: u32,\n    baz: String,\n}\n",
                }
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
        let language_registry = project.read_with(cx, |project, _| {
            project.languages().add(rust_lang());
            project.languages().clone()
        });

        let mut fake_language_servers = language_registry.register_fake_lsp(
            "Rust",
            FakeLspAdapter {
                capabilities: lsp::ServerCapabilities {
                    document_symbol_provider: Some(lsp::OneOf::Left(true)),
                    ..lsp::ServerCapabilities::default()
                },
                initializer: Some(Box::new(|fake_language_server| {
                    fake_language_server
                        .set_request_handler::<lsp::request::DocumentSymbolRequest, _, _>(
                            move |_, _| async move {
                                #[allow(deprecated)]
                                Ok(Some(lsp::DocumentSymbolResponse::Nested(vec![
                                    lsp::DocumentSymbol {
                                        name: "Foo".to_string(),
                                        detail: None,
                                        kind: lsp::SymbolKind::STRUCT,
                                        tags: None,
                                        deprecated: None,
                                        range: lsp::Range::new(
                                            lsp::Position::new(0, 0),
                                            lsp::Position::new(3, 1),
                                        ),
                                        selection_range: lsp::Range::new(
                                            lsp::Position::new(0, 7),
                                            lsp::Position::new(0, 10),
                                        ),
                                        children: Some(vec![
                                            lsp::DocumentSymbol {
                                                name: "bar".to_string(),
                                                detail: None,
                                                kind: lsp::SymbolKind::FIELD,
                                                tags: None,
                                                deprecated: None,
                                                range: lsp::Range::new(
                                                    lsp::Position::new(1, 4),
                                                    lsp::Position::new(1, 13),
                                                ),
                                                selection_range: lsp::Range::new(
                                                    lsp::Position::new(1, 4),
                                                    lsp::Position::new(1, 7),
                                                ),
                                                children: None,
                                            },
                                            lsp::DocumentSymbol {
                                                name: "lsp_only_field".to_string(),
                                                detail: None,
                                                kind: lsp::SymbolKind::FIELD,
                                                tags: None,
                                                deprecated: None,
                                                range: lsp::Range::new(
                                                    lsp::Position::new(2, 4),
                                                    lsp::Position::new(2, 15),
                                                ),
                                                selection_range: lsp::Range::new(
                                                    lsp::Position::new(2, 4),
                                                    lsp::Position::new(2, 7),
                                                ),
                                                children: None,
                                            },
                                        ]),
                                    },
                                ])))
                            },
                        );
                })),
                ..FakeLspAdapter::default()
            },
        );

        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        cx.update(|window, cx| {
            outline_panel.update(cx, |outline_panel, cx| {
                outline_panel.set_active(true, window, cx)
            });
        });

        let _editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from(path!("/root/src/lib.rs")),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..OpenOptions::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .expect("Failed to open Rust source file")
            .downcast::<Editor>()
            .expect("Should open an editor for Rust source file");
        let _fake_language_server = fake_language_servers.next().await.unwrap();
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        // Step 1: tree-sitter outlines by default
        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Foo  <==== selected
  outline: bar
  outline: baz"
                ),
                "Step 1: tree-sitter outlines should be displayed by default"
            );
        });

        // Step 2: Switch to LSP document symbols
        cx.update(|_, cx| {
            settings::SettingsStore::update_global(
                cx,
                |store: &mut settings::SettingsStore, cx| {
                    store.update_user_settings(cx, |settings| {
                        settings.project.all_languages.defaults.document_symbols =
                            Some(settings::DocumentSymbols::On);
                    });
                },
            );
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Foo  <==== selected
  outline: bar
  outline: lsp_only_field"
                ),
                "Step 2: After switching to LSP, should see LSP-provided symbols"
            );
        });

        // Step 3: Switch back to tree-sitter
        cx.update(|_, cx| {
            settings::SettingsStore::update_global(
                cx,
                |store: &mut settings::SettingsStore, cx| {
                    store.update_user_settings(cx, |settings| {
                        settings.project.all_languages.defaults.document_symbols =
                            Some(settings::DocumentSymbols::Off);
                    });
                },
            );
        });
        cx.executor()
            .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
        cx.run_until_parked();

        outline_panel.update(cx, |outline_panel, cx| {
            assert_eq!(
                display_entries(
                    &project,
                    &snapshot(outline_panel, cx),
                    &outline_panel.cached_entries,
                    outline_panel.selected_entry(),
                    cx,
                ),
                indoc!(
                    "
outline: struct Foo  <==== selected
  outline: bar
  outline: baz"
                ),
                "Step 3: tree-sitter outlines should be restored"
            );
        });
    }

    #[gpui::test]
    async fn test_markdown_outline_selection_at_heading_boundaries(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/test",
            json!({
                "doc.md": indoc!("
                    # Section A

                    ## Sub Section A

                    ## Sub Section B

                    # Section B

                ")
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new("/test")], cx).await;
        project.read_with(cx, |project, _| project.languages().add(markdown_lang()));
        let (window, workspace) = add_outline_panel(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        let outline_panel = outline_panel(&workspace, cx);
        outline_panel.update_in(cx, |outline_panel, window, cx| {
            outline_panel.set_active(true, window, cx)
        });

        let editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_abs_path(
                    PathBuf::from("/test/doc.md"),
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    window,
                    cx,
                )
            })
            .await
            .unwrap()
            .downcast::<Editor>()
            .unwrap();

        cx.run_until_parked();

        outline_panel.update_in(cx, |panel, window, cx| {
            panel.update_non_fs_items(window, cx);
            panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
        });

        // Helper function to move the cursor to the first column of a given row
        // and return the selected outline entry's text.
        let move_cursor_and_get_selection =
            |row: u32, cx: &mut VisualTestContext| -> Option<SharedString> {
                cx.update(|window, cx| {
                    editor.update(cx, |editor, cx| {
                        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                            s.select_ranges(Some(
                                language::Point::new(row, 0)..language::Point::new(row, 0),
                            ))
                        });
                    });
                });

                cx.run_until_parked();

                outline_panel.read_with(cx, |panel, _cx| {
                    panel.selected_entry().and_then(|entry| match entry {
                        PanelEntry::Outline(OutlineEntry::Outline(outline)) => {
                            Some(outline.text.clone())
                        }
                        _ => None,
                    })
                })
            };

        assert_eq!(
            move_cursor_and_get_selection(0, cx).as_deref(),
            Some("# Section A"),
            "Cursor at row 0 should select '# Section A'"
        );

        assert_eq!(
            move_cursor_and_get_selection(2, cx).as_deref(),
            Some("## Sub Section A"),
            "Cursor at row 2 should select '## Sub Section A'"
        );

        assert_eq!(
            move_cursor_and_get_selection(4, cx).as_deref(),
            Some("## Sub Section B"),
            "Cursor at row 4 should select '## Sub Section B'"
        );

        assert_eq!(
            move_cursor_and_get_selection(6, cx).as_deref(),
            Some("# Section B"),
            "Cursor at row 6 should select '# Section B'"
        );
    }
}
