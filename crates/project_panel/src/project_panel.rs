#[path = "project_panel/clipboard_system.rs"]
mod clipboard_system;
#[path = "project_panel/diagnostics_menu.rs"]
mod diagnostics_menu;
#[path = "project_panel/drag_actions.rs"]
mod drag_actions;
#[path = "project_panel/editing.rs"]
mod editing;
#[path = "project_panel/entry_details.rs"]
mod entry_details;
#[path = "project_panel/entry_search.rs"]
mod entry_search;
#[path = "project_panel/expansion.rs"]
mod expansion;
#[path = "project_panel/file_operations.rs"]
mod file_operations;
#[path = "project_panel/folded_path_render.rs"]
mod folded_path_render;
#[path = "project_panel/git_actions.rs"]
mod git_actions;
#[path = "project_panel/lifecycle.rs"]
mod lifecycle;
#[path = "project_panel/move_operations.rs"]
mod move_operations;
#[path = "project_panel/navigation.rs"]
mod navigation;
#[path = "project_panel/navigation_search.rs"]
mod navigation_search;
#[path = "project_panel/opening.rs"]
mod opening;
#[path = "project_panel/panel_helpers.rs"]
mod panel_helpers;
pub mod project_panel_settings;
#[path = "project_panel/render_empty.rs"]
mod render_empty;
#[path = "project_panel/render_entry.rs"]
mod render_entry;
#[path = "project_panel/render_entry_events.rs"]
mod render_entry_events;
#[path = "project_panel/render_entry_parts.rs"]
mod render_entry_parts;
#[path = "project_panel/render_panel.rs"]
mod render_panel;
#[path = "project_panel/render_panel_drag.rs"]
mod render_panel_drag;
#[path = "project_panel/selection.rs"]
mod selection;
#[path = "project_panel/sticky_entries.rs"]
mod sticky_entries;
#[path = "project_panel/support.rs"]
mod support;
#[path = "project_panel/system_actions.rs"]
mod system_actions;
#[path = "project_panel/types.rs"]
mod types;
mod undo;
mod utils;
#[path = "project_panel/visible_entries.rs"]
mod visible_entries;

use anyhow::{Context as _, Result};
use client::{ErrorCode, ErrorExt};
use collections::{BTreeSet, HashMap, hash_map};
use command_palette_hooks::CommandPaletteFilter;
use editor::{
    Editor, EditorEvent, MultiBufferOffset,
    items::{
        entry_diagnostic_aware_icon_decoration_and_color,
        entry_diagnostic_aware_icon_name_and_color, entry_git_aware_label_color,
    },
};
use feature_flags::{FeatureFlagAppExt, ProjectPanelUndoRedoFeatureFlag};
use file_icons::FileIcons;
use git;
use git::status::GitSummary;
use git_ui;
use git_ui::file_diff_view::FileDiffView;
use gpui::{
    Action, AnyElement, App, AsyncWindowContext, Bounds, ClipboardEntry as GpuiClipboardEntry,
    ClipboardItem, Context, CursorStyle, DismissEvent, Div, DragMoveEvent, Entity, EventEmitter,
    ExternalPaths, FocusHandle, Focusable, FontWeight, Hsla, InteractiveElement, KeyContext,
    ListHorizontalSizingBehavior, ListSizingBehavior, Modifiers, ModifiersChangedEvent,
    MouseButton, MouseDownEvent, ParentElement, PathPromptOptions, Pixels, Point, PromptLevel,
    Render, ScrollStrategy, Stateful, Styled, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, actions, anchored, deferred, div, hsla, linear_color_stop, linear_gradient,
    point, px, size, transparent_white, uniform_list,
};
use language::DiagnosticSeverity;
use markdown_preview::markdown_preview_view::MarkdownPreviewView;
use mav_actions::{
    project_panel::{Toggle, ToggleFocus},
    workspace::OpenWithSystem,
};
use menu::{Confirm, SelectFirst, SelectLast, SelectNext, SelectPrevious};
use notifications::status_toast::StatusToast;
use project::{
    Entry, EntryKind, Fs, GitEntry, GitEntryRef, GitTraversal, Project, ProjectEntryId,
    ProjectPath, Worktree, WorktreeId,
    git_store::{GitStoreEvent, RepositoryEvent, git_traversal::ChildEntriesGitIter},
    project_settings::GoToDiagnosticSeverityFilter,
};
use project_panel_settings::ProjectPanelSettings;
use rayon::slice::ParallelSliceMut;
use schemars::JsonSchema;
use serde::Deserialize;
use settings::{
    DockSide, ProjectPanelEntrySpacing, Settings, SettingsStore, ShowDiagnostics, ShowIndentGuides,
    update_settings_file,
};
use smallvec::SmallVec;
use std::{
    any::TypeId,
    cell::OnceCell,
    cmp,
    collections::HashSet,
    ops::Neg,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, DecoratedIcon, IconDecoration, IconDecorationKind, IndentGuideColors,
    IndentGuideLayout, Indicator, KeyBinding, ListItem, ListItemSpacing, ProjectEmptyState,
    ScrollAxes, ScrollableHandle, Scrollbars, StickyCandidate, Tooltip, WithScrollbar, prelude::*,
};
use util::{
    ResultExt, TakeUntilExt, TryFutureExt,
    markdown::MarkdownInlineCode,
    maybe,
    paths::{PathStyle, compare_paths},
    rel_path::{RelPath, RelPathBuf},
};
use workspace::{
    DraggedSelection, OpenInTerminal, OpenMode, OpenOptions, OpenVisible, PaneKind,
    PreviewTabsSettings, SelectedEntry, SplitDirection, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::{DetachAndPromptErr, NotifyResultExt, NotifyTaskExt},
};
use worktree::CreatedEntry;

use crate::{
    project_panel_settings::ProjectPanelScrollbarProxy,
    undo::{Change, UndoManager},
};

const PROJECT_PANEL_KEY: &str = "ProjectPanel";
const NEW_ENTRY_ID: ProjectEntryId = ProjectEntryId::MAX;

use support::*;
pub use types::ProjectPanel;
use types::*;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            workspace.toggle_panel_focus::<ProjectPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &Toggle, window, cx| {
            if !workspace.toggle_panel_focus::<ProjectPanel>(window, cx) {
                workspace.close_panel::<ProjectPanel>(window, cx);
            }
        });

        workspace.register_action(|workspace, _: &ToggleHideGitIgnore, _, cx| {
            let fs = workspace.app_state().fs.clone();
            update_settings_file(fs, cx, move |setting, _| {
                setting.project_panel.get_or_insert_default().hide_gitignore = Some(
                    !setting
                        .project_panel
                        .get_or_insert_default()
                        .hide_gitignore
                        .unwrap_or(false),
                );
            })
        });

        workspace.register_action(|workspace, _: &ToggleHideHidden, _, cx| {
            let fs = workspace.app_state().fs.clone();
            update_settings_file(fs, cx, move |setting, _| {
                setting.project_panel.get_or_insert_default().hide_hidden = Some(
                    !setting
                        .project_panel
                        .get_or_insert_default()
                        .hide_hidden
                        .unwrap_or(false),
                );
            })
        });

        workspace.register_action(|workspace, action: &CollapseAllEntries, window, cx| {
            if let Some(panel) = workspace.panel::<ProjectPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.collapse_all_entries(action, window, cx);
                });
            }
        });

        workspace.register_action(|workspace, action: &ExpandAllEntries, window, cx| {
            if let Some(panel) = workspace.panel::<ProjectPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.expand_all_entries(action, window, cx);
                });
            }
        });

        workspace.register_action(|workspace, action: &Rename, window, cx| {
            workspace.open_panel::<ProjectPanel>(window, cx);
            if let Some(panel) = workspace.panel::<ProjectPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    if let Some(first_marked) = panel.marked_entries.first() {
                        let first_marked = *first_marked;
                        panel.marked_entries.clear();
                        panel.selection = Some(first_marked);
                    }
                    panel.rename(action, window, cx);
                });
            }
        });

        workspace.register_action(|workspace, action: &Duplicate, window, cx| {
            workspace.open_panel::<ProjectPanel>(window, cx);
            if let Some(panel) = workspace.panel::<ProjectPanel>(cx) {
                panel.update(cx, |panel, cx| {
                    panel.duplicate(action, window, cx);
                });
            }
        });

        workspace.register_action(|workspace, action: &Delete, window, cx| {
            if let Some(panel) = workspace.panel::<ProjectPanel>(cx) {
                panel.update(cx, |panel, cx| panel.delete(action, window, cx));
            }
        });

        // Forwards `git::FileHistory` to `git_ui::git_graph` when the project
        // panel is the focused source of selection. Lives here (and not in
        // `git_ui`) so that `git_ui` does not need to depend on
        // `project_panel`, which would create a dependency cycle.
        workspace.register_action_renderer(|div, workspace, window, cx| {
            let Some(panel) = workspace.panel::<ProjectPanel>(cx) else {
                return div;
            };
            if !panel.read(cx).focus_handle(cx).contains_focused(window, cx) {
                return div;
            }
            if panel.read(cx).selected_entry_project_path(cx).is_none() {
                return div;
            }
            let workspace = workspace.weak_handle();
            div.capture_action(move |_: &git::FileHistory, window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        let Some(panel) = workspace.panel::<ProjectPanel>(cx) else {
                            return;
                        };
                        let Some(project_path) = panel.read(cx).selected_entry_project_path(cx)
                        else {
                            return;
                        };
                        let Some((repo_id, log_source)) =
                            git_ui::git_graph::resolve_file_history_target_from_project_path(
                                workspace,
                                &project_path,
                                cx,
                            )
                        else {
                            return;
                        };
                        let git_store = workspace.project().read(cx).git_store().clone();
                        git_ui::git_graph::open_or_reuse_graph(
                            workspace, repo_id, git_store, log_source, None, window, cx,
                        );
                    })
                    .log_err();
                cx.stop_propagation();
            })
        });
    })
    .detach();
}

#[derive(Debug)]
pub enum Event {
    OpenedEntry {
        entry_id: ProjectEntryId,
        focus_opened_item: bool,
        allow_preview: bool,
    },
    SplitEntry {
        entry_id: ProjectEntryId,
        allow_preview: bool,
        split_direction: Option<SplitDirection>,
    },
    Focus,
}

struct DraggedProjectEntryView {
    selection: SelectedEntry,
    icon: Option<SharedString>,
    filename: String,
    click_offset: Point<Pixels>,
    selections: Arc<[SelectedEntry]>,
}

struct ItemColors {
    default: Hsla,
    hover: Hsla,
    drag_over: Hsla,
    marked: Hsla,
    focused: Hsla,
}

fn get_item_color(is_sticky: bool, cx: &App) -> ItemColors {
    let colors = cx.theme().colors();

    ItemColors {
        default: if is_sticky {
            colors.panel_overlay_background
        } else {
            colors.editor_background
        },
        hover: if is_sticky {
            colors.panel_overlay_hover
        } else {
            colors.element_hover
        },
        marked: colors.element_selected,
        focused: colors.panel_focused_border,
        drag_over: colors.drop_target_background,
    }
}

#[cfg(test)]
mod project_panel_tests;
mod tests;
