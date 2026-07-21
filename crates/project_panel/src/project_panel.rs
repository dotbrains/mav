#[path = "project_panel/clipboard_system.rs"]
mod clipboard_system;
#[path = "project_panel/diagnostics_menu.rs"]
mod diagnostics_menu;
#[path = "project_panel/drag_actions.rs"]
mod drag_actions;
#[path = "project_panel/editing.rs"]
mod editing;
#[path = "project_panel/entry_search.rs"]
mod entry_search;
#[path = "project_panel/expansion.rs"]
mod expansion;
#[path = "project_panel/file_operations.rs"]
mod file_operations;
#[path = "project_panel/git_actions.rs"]
mod git_actions;
#[path = "project_panel/move_operations.rs"]
mod move_operations;
#[path = "project_panel/navigation.rs"]
mod navigation;
#[path = "project_panel/navigation_search.rs"]
mod navigation_search;
#[path = "project_panel/opening.rs"]
mod opening;
pub mod project_panel_settings;
#[path = "project_panel/selection.rs"]
mod selection;
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

impl ProjectPanel {
    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let path_style = project.read(cx).path_style(cx);
        let project_panel = cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            cx.on_focus(&focus_handle, window, Self::focus_in).detach();

            cx.subscribe_in(
                &git_store,
                window,
                |this, _, event, window, cx| match event {
                    GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::StatusesChanged, _)
                    | GitStoreEvent::RepositoryAdded
                    | GitStoreEvent::RepositoryRemoved(_) => {
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    _ => {}
                },
            )
            .detach();

            cx.subscribe_in(
                &project,
                window,
                |this, project, event, window, cx| match event {
                    project::Event::ActiveEntryChanged(Some(entry_id)) => {
                        if ProjectPanelSettings::get_global(cx).auto_reveal_entries {
                            this.reveal_entry(project.clone(), *entry_id, true, window, cx)
                                .ok();
                        }
                    }
                    project::Event::ActiveEntryChanged(None) => {
                        let is_active_item_file_diff_view = this
                            .workspace
                            .upgrade()
                            .and_then(|ws| ws.read(cx).active_item(cx))
                            .map(|item| {
                                item.act_as_type(TypeId::of::<FileDiffView>(), cx).is_some()
                            })
                            .unwrap_or(false);
                        if !is_active_item_file_diff_view {
                            this.marked_entries.clear();
                        }
                    }
                    project::Event::RevealInProjectPanel(entry_id) => {
                        if let Some(()) = this
                            .reveal_entry(project.clone(), *entry_id, false, window, cx)
                            .log_err()
                        {
                            cx.emit(PanelEvent::Activate);
                        }
                    }
                    project::Event::ActivateProjectPanel => {
                        cx.emit(PanelEvent::Activate);
                    }
                    project::Event::DiskBasedDiagnosticsFinished { .. }
                    | project::Event::DiagnosticsUpdated { .. } => {
                        if ProjectPanelSettings::get_global(cx).show_diagnostics
                            != ShowDiagnostics::Off
                        {
                            this.diagnostic_summary_update = cx.spawn(async move |this, cx| {
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
                    project::Event::WorktreeRemoved(id) => {
                        this.state.expanded_dir_ids.remove(id);
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    project::Event::WorktreeUpdatedEntries(_, _)
                    | project::Event::WorktreeAdded(_)
                    | project::Event::WorktreeOrderChanged => {
                        this.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                    project::Event::ExpandedAllForEntry(worktree_id, entry_id) => {
                        this.synchronously_expand_all_directories(
                            *worktree_id,
                            *entry_id,
                            window,
                            cx,
                        );
                    }
                    _ => {}
                },
            )
            .detach();

            let trash_action = [TypeId::of::<Trash>()];
            let is_remote = project.read(cx).is_remote();

            // Make sure the trash option is never displayed anywhere on remote
            // hosts since they may not support trashing. May want to dynamically
            // detect this in the future.
            if is_remote {
                CommandPaletteFilter::update_global(cx, |filter, _cx| {
                    filter.hide_action_types(&trash_action);
                });
            }

            let filename_editor = cx.new(|cx| Editor::single_line(window, cx));

            cx.subscribe_in(
                &filename_editor,
                window,
                |project_panel, _, editor_event, window, cx| match editor_event {
                    EditorEvent::BufferEdited => {
                        project_panel.populate_validation_error(cx);
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::SelectionsChanged { .. } => {
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::Blurred => {
                        if project_panel
                            .state
                            .edit_state
                            .as_ref()
                            .is_some_and(|state| state.processing_filename.is_none())
                        {
                            match project_panel.confirm_edit(false, window, cx) {
                                Some(task) => {
                                    task.detach_and_notify_err(
                                        project_panel.workspace.clone(),
                                        window,
                                        cx,
                                    );
                                }
                                None => {
                                    project_panel.discard_edit_state(window, cx);
                                }
                            }
                        }
                    }
                    _ => {}
                },
            )
            .detach();

            cx.observe_global::<FileIcons>(|_, cx| {
                cx.notify();
            })
            .detach();

            let mut project_panel_settings = *ProjectPanelSettings::get_global(cx);
            cx.observe_global_in::<SettingsStore>(window, move |this, window, cx| {
                let new_settings = *ProjectPanelSettings::get_global(cx);
                if project_panel_settings != new_settings {
                    if project_panel_settings.hide_gitignore != new_settings.hide_gitignore {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.hide_root != new_settings.hide_root {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.hide_hidden != new_settings.hide_hidden {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sort_mode != new_settings.sort_mode {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sort_order != new_settings.sort_order {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    if project_panel_settings.sticky_scroll && !new_settings.sticky_scroll {
                        this.sticky_items_count = 0;
                    }
                    project_panel_settings = new_settings;
                    this.update_diagnostics(cx);
                    cx.notify();
                }
            })
            .detach();

            let scroll_handle = UniformListScrollHandle::new();
            let weak_project_panel = cx.weak_entity();
            let mut this = Self {
                project: project.clone(),
                hover_scroll_task: None,
                fs: workspace.app_state().fs.clone(),
                focus_handle,
                rendered_entries_len: 0,
                folded_directory_drag_target: None,
                drag_target_entry: None,
                marked_entries: Default::default(),
                selection: None,
                context_menu: None,
                filename_editor,
                clipboard: None,
                _dragged_entry_destination: None,
                workspace: workspace.weak_handle(),
                diagnostics: Default::default(),
                diagnostic_counts: Default::default(),
                diagnostic_summary_update: Task::ready(()),
                scroll_handle,
                mouse_down: false,
                hover_expand_task: None,
                previous_drag_position: None,
                sticky_items_count: 0,
                last_reported_update: Instant::now(),
                state: State {
                    max_width_item_index: None,
                    edit_state: None,
                    temporarily_unfolded_pending_state: None,
                    last_worktree_root_id: Default::default(),
                    visible_entries: Default::default(),
                    ancestors: Default::default(),
                    expanded_dir_ids: Default::default(),
                    unfolded_dir_ids: Default::default(),
                },
                update_visible_entries_task: Default::default(),
                undo_manager: UndoManager::new(workspace.weak_handle(), weak_project_panel, &cx),
            };
            this.update_visible_entries(None, false, false, window, cx);

            this
        });

        cx.subscribe_in(&project_panel, window, {
            let project_panel = project_panel.downgrade();
            move |workspace, _, event, window, cx| match event {
                &Event::OpenedEntry {
                    entry_id,
                    focus_opened_item,
                    allow_preview,
                } => {
                    if let Some(worktree) = project.read(cx).worktree_for_entry(entry_id, cx)
                        && let Some(entry) = worktree.read(cx).entry_for_id(entry_id) {
                            let file_path = entry.path.clone();
                            let worktree_id = worktree.read(cx).id();
                            let entry_id = entry.id;
                            let is_via_ssh = project.read(cx).is_via_remote_server();

                            workspace
                                .open_path_preview_in_tabbed_pane(
                                    ProjectPath {
                                        worktree_id,
                                        path: file_path.clone(),
                                    },
                                    None,
                                    focus_opened_item,
                                    allow_preview,
                                    true,
                                    window, cx,
                                )
                                .detach_and_prompt_err("Failed to open file", window, cx, move |e, _, _| {
                                    match e.error_code() {
                                        ErrorCode::Disconnected => if is_via_ssh {
                                            Some("Disconnected from SSH host".to_string())
                                        } else {
                                            Some("Disconnected from remote project".to_string())
                                        },
                                        ErrorCode::UnsharedItem => Some(format!(
                                            "{} is not shared by the host. This could be because it has been marked as `private`",
                                            file_path.display(path_style)
                                        )),
                                        // See note in worktree.rs where this error originates. Returning Some in this case prevents
                                        // the error popup from saying "Try Again", which is a red herring in this case
                                        ErrorCode::Internal if e.to_string().contains("File is too large to load") => Some(e.to_string()),
                                        _ => None,
                                    }
                                });

                            if let Some(project_panel) = project_panel.upgrade() {
                                // Always select and mark the entry, regardless of whether it is opened or not.
                                project_panel.update(cx, |project_panel, _| {
                                    let entry = SelectedEntry { worktree_id, entry_id };
                                    project_panel.marked_entries.clear();
                                    project_panel.marked_entries.push(entry);
                                    project_panel.selection = Some(entry);
                                });
                                if !focus_opened_item {
                                    let focus_handle = project_panel.read(cx).focus_handle.clone();
                                    window.focus(&focus_handle, cx);
                                }
                            }
                        }
                }
                &Event::SplitEntry {
                    entry_id,
                    allow_preview,
                    split_direction,
                } => {
                    if let Some(worktree) = project.read(cx).worktree_for_entry(entry_id, cx)
                        && let Some(entry) = worktree.read(cx).entry_for_id(entry_id) {
                            workspace
                                .split_path_preview(
                                    ProjectPath {
                                        worktree_id: worktree.read(cx).id(),
                                        path: entry.path.clone(),
                                    },
                                    allow_preview,
                                    split_direction,
                                    window, cx,
                                )
                                .detach_and_log_err(cx);
                        }
                }

                _ => {}
            }
        })
        .detach();

        project_panel
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            ProjectPanel::new(workspace, window, cx)
        })
    }

    fn render_entry(
        &self,
        entry_id: ProjectEntryId,
        details: EntryDetails,
        marked_selections: Arc<[SelectedEntry]>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        const GROUP_NAME: &str = "project_entry";

        let kind = details.kind;
        let is_sticky = details.sticky.is_some();
        let sticky_index = details.sticky.as_ref().map(|this| this.sticky_index);
        let settings = ProjectPanelSettings::get_global(cx);
        let show_editor = details.is_editing && !details.is_processing;

        let selection = SelectedEntry {
            worktree_id: details.worktree_id,
            entry_id,
        };

        let is_marked = self.marked_entries.contains(&selection);
        let is_active = self
            .selection
            .is_some_and(|selection| selection.entry_id == entry_id);

        let file_name = details.filename.clone();

        let mut icon = details.icon.clone();
        if settings.file_icons && show_editor && details.kind.is_file() {
            let filename = self.filename_editor.read(cx).text(cx);
            if filename.len() > 2 {
                icon = FileIcons::get_icon(Path::new(&filename), cx);
            }
        }

        let filename_text_color = details.filename_text_color;
        let diagnostic_severity = details.diagnostic_severity;
        let diagnostic_count = details.diagnostic_count;
        let item_colors = get_item_color(is_sticky, cx);

        let canonical_path = details.canonical_path.clone();
        let path_style = self.project.read(cx).path_style(cx);
        let path = details.path.clone();

        let depth = details.depth;
        let worktree_id = details.worktree_id;

        let bg_color = if is_marked {
            item_colors.marked
        } else {
            item_colors.default
        };

        let bg_hover_color = if is_marked {
            item_colors.marked
        } else {
            item_colors.hover
        };

        let validation_color_and_message = if show_editor {
            match self
                .state
                .edit_state
                .as_ref()
                .map_or(ValidationState::None, |e| e.validation_state.clone())
            {
                ValidationState::Error(msg) => Some((Color::Error.color(cx), msg)),
                ValidationState::Warning(msg) => Some((Color::Warning.color(cx), msg)),
                ValidationState::None => None,
            }
        } else {
            None
        };

        let border_color =
            if !self.mouse_down && is_active && self.focus_handle.contains_focused(window, cx) {
                match validation_color_and_message {
                    Some((color, _)) => color,
                    None => item_colors.focused,
                }
            } else {
                bg_color
            };

        let border_hover_color =
            if !self.mouse_down && is_active && self.focus_handle.contains_focused(window, cx) {
                match validation_color_and_message {
                    Some((color, _)) => color,
                    None => item_colors.focused,
                }
            } else {
                bg_hover_color
            };

        let folded_directory_drag_target = self.folded_directory_drag_target;
        let is_highlighted = {
            if let Some(highlight_entry_id) =
                self.drag_target_entry
                    .as_ref()
                    .and_then(|drag_target| match drag_target {
                        DragTarget::Entry {
                            highlight_entry_id, ..
                        } => Some(*highlight_entry_id),
                        DragTarget::Background => self.state.last_worktree_root_id,
                    })
            {
                // Highlight if same entry or it's children
                if entry_id == highlight_entry_id {
                    true
                } else {
                    maybe!({
                        let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
                        let highlight_entry = worktree.read(cx).entry_for_id(highlight_entry_id)?;
                        Some(path.starts_with(&highlight_entry.path))
                    })
                    .unwrap_or(false)
                }
            } else {
                false
            }
        };
        let git_indicator = settings
            .git_status_indicator
            .then(|| git_status_indicator(details.git_status))
            .flatten();

        let id: ElementId = if is_sticky {
            SharedString::from(format!("project_panel_sticky_item_{}", entry_id.to_usize())).into()
        } else {
            (entry_id.to_proto() as usize).into()
        };

        div()
            .id(id.clone())
            .relative()
            .group(GROUP_NAME)
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_r_2()
            .border_color(border_color)
            .hover(|style| style.bg(bg_hover_color).border_color(border_hover_color))
            .when(is_sticky, |this| this.block_mouse_except_scroll())
            .when(!is_sticky, |this| {
                this.when(
                    is_highlighted && folded_directory_drag_target.is_none(),
                    |this| {
                        this.border_color(transparent_white())
                            .bg(item_colors.drag_over)
                    },
                )
                .when(settings.drag_and_drop, |this| {
                    let path_for_external_paths = path.clone();
                    let path_for_dragged_selection = path.clone();
                    let source_pane = self.workspace.upgrade().and_then(|workspace| {
                        workspace
                            .read(cx)
                            .panel_pane_for_kind(PaneKind::Project, cx)
                            .map(|pane| pane.downgrade())
                    });
                    let dragged_selection = DraggedSelection {
                        active_selection: selection,
                        marked_selections: marked_selections.clone(),
                        source_pane,
                        active_selection_is_file: kind.is_file(),
                    };

                    this.on_drag_move::<ExternalPaths>(cx.listener(
                        move |this, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                            let is_current_target =
                                this.drag_target_entry
                                    .as_ref()
                                    .and_then(|entry| match entry {
                                        DragTarget::Entry {
                                            entry_id: target_id,
                                            ..
                                        } => Some(*target_id),
                                        DragTarget::Background { .. } => None,
                                    })
                                    == Some(entry_id);

                            if !event.bounds.contains(&event.event.position) {
                                // Entry responsible for setting drag target is also responsible to
                                // clear it up after drag is out of bounds
                                if is_current_target {
                                    this.drag_target_entry = None;
                                }
                                return;
                            }

                            if is_current_target {
                                return;
                            }

                            this.marked_entries.clear();

                            let Some((entry_id, highlight_entry_id)) = maybe!({
                                let target_worktree = this
                                    .project
                                    .read(cx)
                                    .worktree_for_id(selection.worktree_id, cx)?
                                    .read(cx);
                                let target_entry =
                                    target_worktree.entry_for_path(&path_for_external_paths)?;
                                let highlight_entry_id = this.highlight_entry_for_external_drag(
                                    target_entry,
                                    target_worktree,
                                )?;
                                Some((target_entry.id, highlight_entry_id))
                            }) else {
                                return;
                            };

                            this.drag_target_entry = Some(DragTarget::Entry {
                                entry_id,
                                highlight_entry_id,
                            });
                        },
                    ))
                    .on_drop(cx.listener(
                        move |this, external_paths: &ExternalPaths, window, cx| {
                            this.drag_target_entry = None;
                            this.hover_scroll_task.take();
                            this.drop_external_files(external_paths.paths(), entry_id, window, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .on_drag_move::<DraggedSelection>(cx.listener(
                        move |this, event: &DragMoveEvent<DraggedSelection>, window, cx| {
                            let is_current_target =
                                this.drag_target_entry
                                    .as_ref()
                                    .and_then(|entry| match entry {
                                        DragTarget::Entry {
                                            entry_id: target_id,
                                            ..
                                        } => Some(*target_id),
                                        DragTarget::Background { .. } => None,
                                    })
                                    == Some(entry_id);

                            if !event.bounds.contains(&event.event.position) {
                                // Entry responsible for setting drag target is also responsible to
                                // clear it up after drag is out of bounds
                                if is_current_target {
                                    this.drag_target_entry = None;
                                }
                                return;
                            }

                            if is_current_target {
                                return;
                            }

                            let drag_state = event.drag(cx);

                            if drag_state.items().count() == 1 {
                                this.marked_entries.clear();
                                this.marked_entries.push(drag_state.active_selection);
                            }

                            let Some((entry_id, highlight_entry_id)) = maybe!({
                                let target_worktree = this
                                    .project
                                    .read(cx)
                                    .worktree_for_id(selection.worktree_id, cx)?
                                    .read(cx);
                                let target_entry =
                                    target_worktree.entry_for_path(&path_for_dragged_selection)?;
                                let highlight_entry_id = this.highlight_entry_for_selection_drag(
                                    target_entry,
                                    target_worktree,
                                    drag_state,
                                    cx,
                                )?;
                                Some((target_entry.id, highlight_entry_id))
                            }) else {
                                return;
                            };

                            this.drag_target_entry = Some(DragTarget::Entry {
                                entry_id,
                                highlight_entry_id,
                            });

                            this.hover_expand_task.take();

                            if !kind.is_dir()
                                || this
                                    .state
                                    .expanded_dir_ids
                                    .get(&details.worktree_id)
                                    .is_some_and(|ids| ids.binary_search(&entry_id).is_ok())
                            {
                                return;
                            }

                            let bounds = event.bounds;
                            this.hover_expand_task =
                                Some(cx.spawn_in(window, async move |this, cx| {
                                    cx.background_executor()
                                        .timer(Duration::from_millis(500))
                                        .await;
                                    this.update_in(cx, |this, window, cx| {
                                        this.hover_expand_task.take();
                                        if this.drag_target_entry.as_ref().and_then(|entry| {
                                            match entry {
                                                DragTarget::Entry {
                                                    entry_id: target_id,
                                                    ..
                                                } => Some(*target_id),
                                                DragTarget::Background { .. } => None,
                                            }
                                        }) == Some(entry_id)
                                            && bounds.contains(&window.mouse_position())
                                        {
                                            this.expand_entry(worktree_id, entry_id, cx);
                                            this.update_visible_entries(
                                                Some((worktree_id, entry_id)),
                                                false,
                                                false,
                                                window,
                                                cx,
                                            );
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                }));
                        },
                    ))
                    .on_drag(dragged_selection, {
                        let active_component =
                            self.state.ancestors.get(&entry_id).and_then(|ancestors| {
                                ancestors.active_component(&details.filename)
                            });
                        move |selection, click_offset, _window, cx| {
                            let filename = active_component
                                .as_ref()
                                .unwrap_or_else(|| &details.filename);
                            cx.new(|_| DraggedProjectEntryView {
                                icon: details.icon.clone(),
                                filename: filename.clone(),
                                click_offset,
                                selection: selection.active_selection,
                                selections: selection.marked_selections.clone(),
                            })
                        }
                    })
                    .on_drop(cx.listener(
                        move |this, selections: &DraggedSelection, window, cx| {
                            this.drag_target_entry = None;
                            this.hover_scroll_task.take();
                            this.hover_expand_task.take();
                            if folded_directory_drag_target.is_some() {
                                return;
                            }
                            this.drag_onto(selections, entry_id, kind.is_file(), window, cx);
                        },
                    ))
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.mouse_down = true;
                    cx.propagate();
                }),
            )
            .on_click(
                cx.listener(move |project_panel, event: &gpui::ClickEvent, window, cx| {
                    if event.is_right_click() || show_editor {
                        return;
                    }
                    if event.standard_click() {
                        project_panel.mouse_down = false;
                    }
                    cx.stop_propagation();

                    if let Some(selection) =
                        project_panel.selection.filter(|_| event.modifiers().shift)
                    {
                        let current_selection = project_panel.index_for_selection(selection);
                        let clicked_entry = SelectedEntry {
                            entry_id,
                            worktree_id,
                        };
                        let target_selection = project_panel.index_for_selection(clicked_entry);
                        if let Some(((_, _, source_index), (_, _, target_index))) =
                            current_selection.zip(target_selection)
                        {
                            let range_start = source_index.min(target_index);
                            let range_end = source_index.max(target_index) + 1;
                            let mut new_selections = Vec::new();
                            project_panel.for_each_visible_entry(
                                range_start..range_end,
                                window,
                                cx,
                                &mut |entry_id, details, _, _| {
                                    new_selections.push(SelectedEntry {
                                        entry_id,
                                        worktree_id: details.worktree_id,
                                    });
                                },
                            );

                            for selection in &new_selections {
                                if !project_panel.marked_entries.contains(selection) {
                                    project_panel.marked_entries.push(*selection);
                                }
                            }

                            project_panel.selection = Some(clicked_entry);
                            if !project_panel.marked_entries.contains(&clicked_entry) {
                                project_panel.marked_entries.push(clicked_entry);
                            }
                        }
                    } else if event.modifiers().secondary() {
                        if event.click_count() > 1 {
                            project_panel.split_entry(entry_id, false, None, cx);
                        } else {
                            project_panel.selection = Some(selection);
                            if let Some(position) = project_panel
                                .marked_entries
                                .iter()
                                .position(|e| *e == selection)
                            {
                                project_panel.marked_entries.remove(position);
                            } else {
                                project_panel.marked_entries.push(selection);
                            }
                        }
                    } else if kind.is_dir() {
                        project_panel.marked_entries.clear();
                        if is_sticky
                            && let Some((_, _, index)) =
                                project_panel.index_for_entry(entry_id, worktree_id)
                        {
                            project_panel
                                .scroll_handle
                                .scroll_to_item_strict_with_offset(
                                    index,
                                    ScrollStrategy::Top,
                                    sticky_index.unwrap_or(0),
                                );
                            cx.notify();
                            // move down by 1px so that clicked item
                            // don't count as sticky anymore
                            cx.on_next_frame(window, |_, window, cx| {
                                cx.on_next_frame(window, |this, _, cx| {
                                    let mut offset = this.scroll_handle.offset();
                                    offset.y += px(1.);
                                    this.scroll_handle.set_offset(offset);
                                    cx.notify();
                                });
                            });
                            return;
                        }
                        if event.modifiers().alt {
                            project_panel.toggle_expand_all(entry_id, window, cx);
                        } else {
                            project_panel.toggle_expanded(entry_id, window, cx);
                        }
                    } else {
                        let preview_tabs_enabled =
                            PreviewTabsSettings::get_global(cx).enable_preview_from_project_panel;
                        let click_count = event.click_count();
                        let focus_opened_item = click_count > 1;
                        let allow_preview = preview_tabs_enabled && click_count == 1;
                        project_panel.open_entry(entry_id, focus_opened_item, allow_preview, cx);
                    }
                }),
            )
            .child(
                ListItem::new(id)
                    .indent_level(depth)
                    .indent_step_size(px(settings.indent_size))
                    .spacing(match settings.entry_spacing {
                        ProjectPanelEntrySpacing::Comfortable => ListItemSpacing::Dense,
                        ProjectPanelEntrySpacing::Standard => ListItemSpacing::ExtraDense,
                    })
                    .selectable(false)
                    .when(
                        canonical_path.is_some()
                            || diagnostic_count.is_some()
                            || git_indicator.is_some(),
                        |this| {
                            let symlink_element = canonical_path.map(|path| {
                                div()
                                    .id("symlink_icon")
                                    .tooltip(move |_window, cx| {
                                        Tooltip::with_meta(
                                            path.to_string_lossy().into_owned(),
                                            None,
                                            "Symbolic Link",
                                            cx,
                                        )
                                    })
                                    .child(
                                        Icon::new(IconName::ArrowUpRight)
                                            .size(IconSize::Indicator)
                                            .color(filename_text_color),
                                    )
                            });
                            this.end_slot::<AnyElement>(
                                h_flex()
                                    .gap_1()
                                    .flex_none()
                                    .pr_3()
                                    .when_some(diagnostic_count, |this, count| {
                                        this.when(count.error_count > 0, |this| {
                                            this.child(
                                                Label::new(count.capped_error_count())
                                                    .size(LabelSize::Small)
                                                    .color(Color::Error),
                                            )
                                        })
                                        .when(
                                            count.warning_count > 0,
                                            |this| {
                                                this.child(
                                                    Label::new(count.capped_warning_count())
                                                        .size(LabelSize::Small)
                                                        .color(Color::Warning),
                                                )
                                            },
                                        )
                                    })
                                    .when_some(git_indicator, |this, (label, color)| {
                                        let git_indicator = if kind.is_dir() {
                                            Indicator::dot()
                                                .color(Color::Custom(color.color(cx).opacity(0.5)))
                                                .into_any_element()
                                        } else {
                                            Label::new(label)
                                                .size(LabelSize::Small)
                                                .color(color)
                                                .into_any_element()
                                        };

                                        this.child(git_indicator)
                                    })
                                    .when_some(symlink_element, |this, el| this.child(el))
                                    .into_any_element(),
                            )
                        },
                    )
                    .child(if let Some(icon) = &icon {
                        if let Some((_, decoration_color)) =
                            entry_diagnostic_aware_icon_decoration_and_color(diagnostic_severity)
                        {
                            let is_warning = diagnostic_severity
                                .map(|severity| matches!(severity, DiagnosticSeverity::WARNING))
                                .unwrap_or(false);
                            div().child(
                                DecoratedIcon::new(
                                    Icon::from_path(icon.clone()).color(Color::Muted),
                                    Some(
                                        IconDecoration::new(
                                            if kind.is_file() {
                                                if is_warning {
                                                    IconDecorationKind::Triangle
                                                } else {
                                                    IconDecorationKind::X
                                                }
                                            } else {
                                                IconDecorationKind::Dot
                                            },
                                            bg_color,
                                            cx,
                                        )
                                        .group_name(Some(GROUP_NAME.into()))
                                        .knockout_hover_color(bg_hover_color)
                                        .color(decoration_color.color(cx))
                                        .position(Point {
                                            x: px(-2.),
                                            y: px(-2.),
                                        }),
                                    ),
                                )
                                .into_any_element(),
                            )
                        } else {
                            h_flex().child(Icon::from_path(icon.to_string()).color(Color::Muted))
                        }
                    } else if let Some((icon_name, color)) =
                        entry_diagnostic_aware_icon_name_and_color(diagnostic_severity)
                    {
                        h_flex()
                            .size(IconSize::default().rems())
                            .child(Icon::new(icon_name).color(color).size(IconSize::Small))
                    } else {
                        h_flex()
                            .size(IconSize::default().rems())
                            .invisible()
                            .flex_none()
                    })
                    .child(if show_editor {
                        h_flex().h_6().w_full().child(self.filename_editor.clone())
                    } else {
                        h_flex()
                            .h_6()
                            .map(|this| match self.state.ancestors.get(&entry_id) {
                                Some(folded_ancestors) => {
                                    this.children(self.render_folder_elements(
                                        folded_ancestors,
                                        entry_id,
                                        file_name,
                                        path_style,
                                        is_sticky,
                                        kind.is_file(),
                                        is_active || is_marked,
                                        settings.drag_and_drop,
                                        settings.bold_folder_labels,
                                        item_colors.drag_over,
                                        folded_directory_drag_target,
                                        filename_text_color,
                                        cx,
                                    ))
                                }

                                None => this.child(
                                    Label::new(file_name)
                                        .single_line()
                                        .color(filename_text_color)
                                        .when(
                                            settings.bold_folder_labels && kind.is_dir(),
                                            |this| this.weight(FontWeight::SEMIBOLD),
                                        )
                                        .into_any_element(),
                                ),
                            })
                    })
                    .on_secondary_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, window, cx| {
                            // Stop propagation to prevent the catch-all context menu for the project
                            // panel from being deployed.
                            cx.stop_propagation();
                            // Some context menu actions apply to all marked entries. If the user
                            // right-clicks on an entry that is not marked, they may not realize the
                            // action applies to multiple entries. To avoid inadvertent changes, all
                            // entries are unmarked.
                            if !this.marked_entries.contains(&selection) {
                                this.marked_entries.clear();
                            }
                            this.deploy_context_menu(event.position, entry_id, window, cx);
                        },
                    ))
                    .overflow_x(),
            )
            .when_some(validation_color_and_message, |this, (color, message)| {
                this.relative().child(deferred(
                    div()
                        .occlude()
                        .absolute()
                        .top_full()
                        .left(px(-1.)) // Used px over rem so that it doesn't change with font size
                        .right(px(-0.5))
                        .py_1()
                        .px_2()
                        .border_1()
                        .border_color(color)
                        .bg(cx.theme().colors().background)
                        .child(
                            Label::new(message)
                                .color(Color::from(color))
                                .size(LabelSize::Small),
                        ),
                ))
            })
    }

    fn render_folder_elements(
        &self,
        folded_ancestors: &FoldedAncestors,
        entry_id: ProjectEntryId,
        file_name: String,
        path_style: PathStyle,
        is_sticky: bool,
        is_file: bool,
        is_active_or_marked: bool,
        drag_and_drop_enabled: bool,
        bold_folder_labels: bool,
        drag_over_color: Hsla,
        folded_directory_drag_target: Option<FoldedDirectoryDragTarget>,
        filename_text_color: Color,
        cx: &Context<Self>,
    ) -> impl Iterator<Item = AnyElement> {
        let components = Path::new(&file_name)
            .components()
            .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let active_index = folded_ancestors.active_index();
        let components_len = components.len();
        let delimiter = SharedString::new(path_style.primary_separator());

        let path_component_elements =
            components
                .into_iter()
                .enumerate()
                .map(move |(index, component)| {
                    div()
                        .id(SharedString::from(format!(
                            "project_panel_path_component_{}_{index}",
                            entry_id.to_usize()
                        )))
                        .when(index == 0, |this| this.ml_neg_0p5())
                        .px_0p5()
                        .rounded_xs()
                        .hover(|style| style.bg(cx.theme().colors().element_active))
                        .when(!is_sticky, |div| {
                            div.when(index != components_len - 1, |div| {
                                let target_entry_id = folded_ancestors
                                    .ancestors
                                    .get(components_len - 1 - index)
                                    .cloned();
                                div.when(drag_and_drop_enabled, |div| {
                                    div.on_drag_move(cx.listener(
                                        move |this,
                                              event: &DragMoveEvent<DraggedSelection>,
                                              _,
                                              _| {
                                            if event.bounds.contains(&event.event.position) {
                                                this.folded_directory_drag_target =
                                                    Some(FoldedDirectoryDragTarget {
                                                        entry_id,
                                                        index,
                                                        is_delimiter_target: false,
                                                    });
                                            } else {
                                                let is_current_target = this
                                                    .folded_directory_drag_target
                                                    .as_ref()
                                                    .is_some_and(|target| {
                                                        target.entry_id == entry_id
                                                            && target.index == index
                                                            && !target.is_delimiter_target
                                                    });
                                                if is_current_target {
                                                    this.folded_directory_drag_target = None;
                                                }
                                            }
                                        },
                                    ))
                                    .on_drop(cx.listener(
                                        move |this, selections: &DraggedSelection, window, cx| {
                                            this.hover_scroll_task.take();
                                            this.drag_target_entry = None;
                                            this.folded_directory_drag_target = None;
                                            if let Some(target_entry_id) = target_entry_id {
                                                this.drag_onto(
                                                    selections,
                                                    target_entry_id,
                                                    is_file,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ))
                                    .when(
                                        folded_directory_drag_target.is_some_and(|target| {
                                            target.entry_id == entry_id && target.index == index
                                        }),
                                        |this| this.bg(drag_over_color),
                                    )
                                })
                            })
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(folds) = this.state.ancestors.get_mut(&entry_id) {
                                    if folds.set_active_index(index) {
                                        cx.notify();
                                    }
                                }
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                if let Some(folds) = this.state.ancestors.get_mut(&entry_id) {
                                    if folds.set_active_index(index) {
                                        cx.notify();
                                    }
                                }
                            }),
                        )
                        .child(
                            Label::new(component)
                                .single_line()
                                .color(filename_text_color)
                                .when(bold_folder_labels && !is_file, |this| {
                                    this.weight(FontWeight::SEMIBOLD)
                                })
                                .when(index == active_index && is_active_or_marked, |this| {
                                    this.underline()
                                }),
                        )
                        .into_any()
                });

        let mut separator_index = 0;
        itertools::intersperse_with(path_component_elements, move || {
            separator_index += 1;
            self.render_entry_path_separator(
                entry_id,
                separator_index,
                components_len,
                is_sticky,
                is_file,
                drag_and_drop_enabled,
                filename_text_color,
                &delimiter,
                folded_ancestors,
                cx,
            )
            .into_any()
        })
    }

    fn render_entry_path_separator(
        &self,
        entry_id: ProjectEntryId,
        index: usize,
        components_len: usize,
        is_sticky: bool,
        is_file: bool,
        drag_and_drop_enabled: bool,
        filename_text_color: Color,
        delimiter: &SharedString,
        folded_ancestors: &FoldedAncestors,
        cx: &Context<Self>,
    ) -> Div {
        let delimiter_target_index = index - 1;
        let target_entry_id = folded_ancestors
            .ancestors
            .get(components_len - 1 - delimiter_target_index)
            .cloned();
        div()
            .when(!is_sticky, |div| {
                div.when(drag_and_drop_enabled, |div| {
                    div.on_drop(cx.listener(
                        move |this, selections: &DraggedSelection, window, cx| {
                            this.hover_scroll_task.take();
                            this.drag_target_entry = None;
                            this.folded_directory_drag_target = None;
                            if let Some(target_entry_id) = target_entry_id {
                                this.drag_onto(selections, target_entry_id, is_file, window, cx);
                            }
                        },
                    ))
                    .on_drag_move(cx.listener(
                        move |this, event: &DragMoveEvent<DraggedSelection>, _, _| {
                            if event.bounds.contains(&event.event.position) {
                                this.folded_directory_drag_target =
                                    Some(FoldedDirectoryDragTarget {
                                        entry_id,
                                        index: delimiter_target_index,
                                        is_delimiter_target: true,
                                    });
                            } else {
                                let is_current_target =
                                    this.folded_directory_drag_target.is_some_and(|target| {
                                        target.entry_id == entry_id
                                            && target.index == delimiter_target_index
                                            && target.is_delimiter_target
                                    });
                                if is_current_target {
                                    this.folded_directory_drag_target = None;
                                }
                            }
                        },
                    ))
                })
            })
            .child(
                Label::new(delimiter.clone())
                    .single_line()
                    .color(filename_text_color),
            )
    }

    fn details_for_entry(
        &self,
        entry: &Entry,
        worktree_id: WorktreeId,
        root_name: &RelPath,
        entries_paths: &HashSet<Arc<RelPath>>,
        git_status: GitSummary,
        sticky: Option<StickyDetails>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EntryDetails {
        let (show_file_icons, show_folder_icons) = {
            let settings = ProjectPanelSettings::get_global(cx);
            (settings.file_icons, settings.folder_icons)
        };

        let expanded_entry_ids = self
            .state
            .expanded_dir_ids
            .get(&worktree_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let is_expanded = expanded_entry_ids.binary_search(&entry.id).is_ok();

        let icon = match entry.kind {
            EntryKind::File => {
                if show_file_icons {
                    FileIcons::get_icon(entry.path.as_std_path(), cx)
                } else {
                    None
                }
            }
            _ => {
                if show_folder_icons {
                    FileIcons::get_folder_icon(is_expanded, entry.path.as_std_path(), cx)
                } else {
                    FileIcons::get_chevron_icon(is_expanded, cx)
                }
            }
        };

        let path_style = self.project.read(cx).path_style(cx);
        let (depth, difference) =
            ProjectPanel::calculate_depth_and_difference(entry, entries_paths);

        let filename = if difference > 1 {
            entry
                .path
                .last_n_components(difference)
                .map_or(String::new(), |suffix| {
                    suffix.display(path_style).to_string()
                })
        } else {
            entry
                .path
                .file_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| root_name.as_unix_str().to_string())
        };

        let selection = SelectedEntry {
            worktree_id,
            entry_id: entry.id,
        };
        let is_marked = self.marked_entries.contains(&selection);
        let is_selected = self.selection == Some(selection);

        let diagnostic_severity = self
            .diagnostics
            .get(&(worktree_id, entry.path.clone()))
            .cloned();

        let diagnostic_count = self
            .diagnostic_counts
            .get(&(worktree_id, entry.path.clone()))
            .copied();

        let filename_text_color =
            entry_git_aware_label_color(git_status, entry.is_ignored, is_marked);

        let is_cut = self
            .clipboard
            .as_ref()
            .is_some_and(|e| e.is_cut() && e.items().contains(&selection));

        EntryDetails {
            filename,
            icon,
            path: entry.path.clone(),
            depth,
            kind: entry.kind,
            is_ignored: entry.is_ignored,
            is_expanded,
            is_selected,
            is_marked,
            is_editing: false,
            is_processing: false,
            is_cut,
            sticky,
            filename_text_color,
            diagnostic_severity,
            diagnostic_count,
            git_status,
            is_private: entry.is_private,
            worktree_id,
            canonical_path: entry.canonical_path.clone(),
        }
    }

    fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("ProjectPanel");
        dispatch_context.add("menu");

        let identifier = if self.filename_editor.focus_handle(cx).is_focused(window) {
            "editing"
        } else {
            "not_editing"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }

    fn reveal_entry(
        &mut self,
        project: Entity<Project>,
        entry_id: ProjectEntryId,
        skip_ignored: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let worktree = project
            .read(cx)
            .worktree_for_entry(entry_id, cx)
            .context("can't reveal a non-existent entry in the project panel")?;
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        let is_ignored = worktree
            .entry_for_id(entry_id)
            .is_none_or(|entry| entry.is_ignored && !entry.is_always_included);
        if skip_ignored && is_ignored {
            if self.index_for_entry(entry_id, worktree_id).is_none() {
                anyhow::bail!("can't reveal an ignored entry in the project panel");
            }

            self.selection = Some(SelectedEntry {
                worktree_id,
                entry_id,
            });
            self.marked_entries.clear();
            self.marked_entries.push(SelectedEntry {
                worktree_id,
                entry_id,
            });
            self.autoscroll(cx);
            cx.notify();
            return Ok(());
        }
        let is_active_item_file_diff_view = self
            .workspace
            .upgrade()
            .and_then(|ws| ws.read(cx).active_item(cx))
            .map(|item| item.act_as_type(TypeId::of::<FileDiffView>(), cx).is_some())
            .unwrap_or(false);
        if is_active_item_file_diff_view {
            return Ok(());
        }

        self.expand_entry(worktree_id, entry_id, cx);
        self.update_visible_entries(Some((worktree_id, entry_id)), false, true, window, cx);
        self.marked_entries.clear();
        self.marked_entries.push(SelectedEntry {
            worktree_id,
            entry_id,
        });
        cx.notify();
        Ok(())
    }

    fn find_active_indent_guide(
        &self,
        indent_guides: &[IndentGuideLayout],
        cx: &App,
    ) -> Option<usize> {
        let (worktree, entry) = self.selected_entry(cx)?;

        // Find the parent entry of the indent guide, this will either be the
        // expanded folder we have selected, or the parent of the currently
        // selected file/collapsed directory
        let mut entry = entry;
        loop {
            let is_expanded_dir = entry.is_dir()
                && self
                    .state
                    .expanded_dir_ids
                    .get(&worktree.id())
                    .map(|ids| ids.binary_search(&entry.id).is_ok())
                    .unwrap_or(false);
            if is_expanded_dir {
                break;
            }
            entry = worktree.entry_for_path(&entry.path.parent()?)?;
        }

        let (active_indent_range, depth) = {
            let (worktree_ix, child_offset, ix) = self.index_for_entry(entry.id, worktree.id())?;
            let child_paths = &self.state.visible_entries[worktree_ix].entries;
            let mut child_count = 0;
            let depth = entry.path.ancestors().count();
            while let Some(entry) = child_paths.get(child_offset + child_count + 1) {
                if entry.path.ancestors().count() <= depth {
                    break;
                }
                child_count += 1;
            }

            let start = ix + 1;
            let end = start + child_count;

            let visible_worktree = &self.state.visible_entries[worktree_ix];
            let visible_worktree_entries = visible_worktree.index.get_or_init(|| {
                visible_worktree
                    .entries
                    .iter()
                    .map(|e| e.path.clone())
                    .collect()
            });

            // Calculate the actual depth of the entry, taking into account that directories can be auto-folded.
            let (depth, _) = Self::calculate_depth_and_difference(entry, visible_worktree_entries);
            (start..end, depth)
        };

        let candidates = indent_guides
            .iter()
            .enumerate()
            .filter(|(_, indent_guide)| indent_guide.offset.x == depth);

        for (i, indent) in candidates {
            // Find matches that are either an exact match, partially on screen, or inside the enclosing indent
            if active_indent_range.start <= indent.offset.y + indent.length
                && indent.offset.y <= active_indent_range.end
            {
                return Some(i);
            }
        }
        None
    }

    fn render_sticky_entries(
        &self,
        child: StickyProjectPanelCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SmallVec<[AnyElement; 8]> {
        let project = self.project.read(cx);

        let Some((worktree_id, entry_ref)) = self.entry_at_index(child.index) else {
            return SmallVec::new();
        };

        let Some(visible) = self
            .state
            .visible_entries
            .iter()
            .find(|worktree| worktree.worktree_id == worktree_id)
        else {
            return SmallVec::new();
        };

        let Some(worktree) = project.worktree_for_id(worktree_id, cx) else {
            return SmallVec::new();
        };
        let worktree = worktree.read(cx).snapshot();

        let paths = visible
            .index
            .get_or_init(|| visible.entries.iter().map(|e| e.path.clone()).collect());

        let mut sticky_parents = Vec::new();
        let mut current_path = entry_ref.path.clone();

        'outer: loop {
            if let Some(parent_path) = current_path.parent() {
                for ancestor_path in parent_path.ancestors() {
                    if paths.contains(ancestor_path)
                        && let Some(parent_entry) = worktree.entry_for_path(ancestor_path)
                    {
                        sticky_parents.push(parent_entry.clone());
                        current_path = parent_entry.path.clone();
                        continue 'outer;
                    }
                }
            }
            break 'outer;
        }

        if sticky_parents.is_empty() {
            return SmallVec::new();
        }

        sticky_parents.reverse();

        let panel_settings = ProjectPanelSettings::get_global(cx);
        let git_status_enabled = panel_settings.git_status;
        let root_name = worktree.root_name();

        let git_summaries_by_id = if git_status_enabled {
            visible
                .entries
                .iter()
                .map(|e| (e.id, e.git_summary))
                .collect::<HashMap<_, _>>()
        } else {
            Default::default()
        };

        // already checked if non empty above
        let last_item_index = sticky_parents.len() - 1;
        let marked_selections: Arc<[SelectedEntry]> = Arc::from(self.marked_entries.clone());
        sticky_parents
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let git_status = git_summaries_by_id
                    .get(&entry.id)
                    .copied()
                    .unwrap_or_default();
                let sticky_details = Some(StickyDetails {
                    sticky_index: index,
                });
                let details = self.details_for_entry(
                    entry,
                    worktree_id,
                    root_name,
                    paths,
                    git_status,
                    sticky_details,
                    window,
                    cx,
                );
                self.render_entry(
                    entry.id,
                    details,
                    Arc::clone(&marked_selections),
                    window,
                    cx,
                )
                .when(index == last_item_index, |this| {
                    let shadow_color_top = hsla(0.0, 0.0, 0.0, 0.1);
                    let shadow_color_bottom = hsla(0.0, 0.0, 0.0, 0.);
                    let sticky_shadow = div()
                        .absolute()
                        .left_0()
                        .bottom_neg_1p5()
                        .h_1p5()
                        .w_full()
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(shadow_color_top, 1.),
                            linear_color_stop(shadow_color_bottom, 0.),
                        ));
                    this.child(sticky_shadow)
                })
                .into_any()
            })
            .collect()
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_worktree = !self.state.visible_entries.is_empty();
        let project = self.project.read(cx);
        let panel_settings = ProjectPanelSettings::get_global(cx);
        let indent_size = panel_settings.indent_size;
        let show_indent_guides = panel_settings.indent_guides.show == ShowIndentGuides::Always;
        let horizontal_scroll = panel_settings.scrollbar.horizontal_scroll;
        let show_sticky_entries = {
            if panel_settings.sticky_scroll {
                let is_scrollable = self.scroll_handle.is_scrollable();
                let is_scrolled = self.scroll_handle.offset().y < px(0.);
                is_scrollable && is_scrolled
            } else {
                false
            }
        };

        let is_local = project.is_local();

        if has_worktree {
            let item_count = self
                .state
                .visible_entries
                .iter()
                .map(|worktree| worktree.entries.len())
                .sum();

            fn handle_drag_move<T: 'static>(
                this: &mut ProjectPanel,
                e: &DragMoveEvent<T>,
                window: &mut Window,
                cx: &mut Context<ProjectPanel>,
            ) {
                if let Some(previous_position) = this.previous_drag_position {
                    // Refresh cursor only when an actual drag happens,
                    // because modifiers are not updated when the cursor is not moved.
                    if e.event.position != previous_position {
                        this.refresh_drag_cursor_style(&e.event.modifiers, window, cx);
                    }
                }
                this.previous_drag_position = Some(e.event.position);

                if !e.bounds.contains(&e.event.position) {
                    this.drag_target_entry = None;
                    return;
                }
                this.hover_scroll_task.take();
                let panel_height = e.bounds.size.height;
                if panel_height <= px(0.) {
                    return;
                }

                let event_offset = e.event.position.y - e.bounds.origin.y;
                // How far along in the project panel is our cursor? (0. is the top of a list, 1. is the bottom)
                let hovered_region_offset = event_offset / panel_height;

                // We want the scrolling to be a bit faster when the cursor is closer to the edge of a list.
                // These pixels offsets were picked arbitrarily.
                let vertical_scroll_offset = if hovered_region_offset <= 0.05 {
                    8.
                } else if hovered_region_offset <= 0.15 {
                    5.
                } else if hovered_region_offset >= 0.95 {
                    -8.
                } else if hovered_region_offset >= 0.85 {
                    -5.
                } else {
                    return;
                };
                let adjustment = point(px(0.), px(vertical_scroll_offset));
                this.hover_scroll_task = Some(cx.spawn_in(window, async move |this, cx| {
                    loop {
                        let should_stop_scrolling = this
                            .update(cx, |this, cx| {
                                this.hover_scroll_task.as_ref()?;
                                let handle = this.scroll_handle.0.borrow_mut();
                                let offset = handle.base_handle.offset();

                                handle.base_handle.set_offset(offset + adjustment);
                                cx.notify();
                                Some(())
                            })
                            .ok()
                            .flatten()
                            .is_some();
                        if should_stop_scrolling {
                            return;
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                }));
            }
            h_flex()
                .id("project-panel")
                .group("project-panel")
                .when(panel_settings.drag_and_drop, |this| {
                    this.on_drag_move(cx.listener(handle_drag_move::<ExternalPaths>))
                        .on_drag_move(cx.listener(handle_drag_move::<DraggedSelection>))
                })
                .size_full()
                .bg(cx.theme().colors().editor_background)
                .relative()
                .on_modifiers_changed(cx.listener(
                    |this, event: &ModifiersChangedEvent, window, cx| {
                        this.refresh_drag_cursor_style(&event.modifiers, window, cx);
                    },
                ))
                .key_context(self.dispatch_context(window, cx))
                .on_action(cx.listener(Self::scroll_up))
                .on_action(cx.listener(Self::scroll_down))
                .on_action(cx.listener(Self::scroll_cursor_center))
                .on_action(cx.listener(Self::scroll_cursor_top))
                .on_action(cx.listener(Self::scroll_cursor_bottom))
                .on_action(cx.listener(Self::select_next))
                .on_action(cx.listener(Self::select_previous))
                .on_action(cx.listener(Self::select_first))
                .on_action(cx.listener(Self::select_last))
                .on_action(cx.listener(Self::select_parent))
                .on_action(cx.listener(Self::select_next_git_entry))
                .on_action(cx.listener(Self::select_prev_git_entry))
                .on_action(cx.listener(Self::select_next_diagnostic))
                .on_action(cx.listener(Self::select_prev_diagnostic))
                .on_action(cx.listener(Self::select_next_directory))
                .on_action(cx.listener(Self::select_prev_directory))
                .on_action(cx.listener(Self::expand_selected_entry))
                .on_action(cx.listener(Self::collapse_selected_entry))
                .on_action(cx.listener(Self::collapse_all_entries))
                .on_action(cx.listener(Self::expand_all_entries))
                .on_action(cx.listener(Self::collapse_selected_entry_and_children))
                .on_action(cx.listener(Self::expand_selected_entry_and_children))
                .on_action(cx.listener(Self::open))
                .on_action(cx.listener(Self::open_permanent))
                .on_action(cx.listener(Self::open_split_vertical))
                .on_action(cx.listener(Self::open_split_horizontal))
                .on_action(cx.listener(Self::open_markdown_preview))
                .on_action(cx.listener(Self::confirm))
                .on_action(cx.listener(Self::cancel))
                .on_action(cx.listener(Self::copy_path))
                .on_action(cx.listener(Self::copy_relative_path))
                .on_action(cx.listener(Self::new_search_in_directory))
                .on_action(cx.listener(Self::unfold_directory))
                .on_action(cx.listener(Self::fold_directory))
                .on_action(cx.listener(Self::remove_from_project))
                .on_action(cx.listener(Self::compare_marked_files))
                .when(cx.has_flag::<ProjectPanelUndoRedoFeatureFlag>(), |el| {
                    el.on_action(cx.listener(Self::undo))
                        .on_action(cx.listener(Self::redo))
                })
                .when(!project.is_read_only(cx), |el| {
                    el.on_action(cx.listener(Self::new_file))
                        .on_action(cx.listener(Self::new_directory))
                        .on_action(cx.listener(Self::rename))
                        .on_action(cx.listener(Self::delete))
                        .on_action(cx.listener(Self::cut))
                        .on_action(cx.listener(Self::copy))
                        .on_action(cx.listener(Self::paste))
                        .on_action(cx.listener(Self::duplicate))
                        .on_action(cx.listener(Self::restore_file))
                        .on_action(cx.listener(Self::add_to_gitignore))
                        .on_action(cx.listener(Self::add_to_git_info_exclude))
                        .when(!project.is_remote(), |el| {
                            el.on_action(cx.listener(Self::trash))
                        })
                })
                .when(
                    project.is_local() || project.is_via_wsl_with_host_interop(cx),
                    |el| {
                        el.on_action(cx.listener(Self::reveal_in_finder))
                            .on_action(cx.listener(Self::open_system))
                            .on_action(cx.listener(Self::open_in_terminal))
                    },
                )
                .when(project.is_via_remote_server(), |el| {
                    el.on_action(cx.listener(Self::open_in_terminal))
                        .on_action(cx.listener(Self::download_from_remote))
                })
                .track_focus(&self.focus_handle(cx))
                .child(
                    v_flex()
                        .child(
                            uniform_list("entries", item_count, {
                                cx.processor(|this, range: Range<usize>, window, cx| {
                                    this.rendered_entries_len = range.end - range.start;
                                    let mut items = Vec::with_capacity(this.rendered_entries_len);
                                    let marked_selections: Arc<[SelectedEntry]> =
                                        Arc::from(this.marked_entries.clone());
                                    this.for_each_visible_entry(
                                        range,
                                        window,
                                        cx,
                                        &mut |id, details, window, cx| {
                                            items.push(this.render_entry(
                                                id,
                                                details,
                                                Arc::clone(&marked_selections),
                                                window,
                                                cx,
                                            ));
                                        },
                                    );
                                    items
                                })
                            })
                            .when(show_indent_guides, |list| {
                                list.with_decoration(
                                    ui::indent_guides(
                                        px(indent_size),
                                        IndentGuideColors::panel(cx),
                                    )
                                    .with_compute_indents_fn(
                                        cx.entity(),
                                        |this, range, window, cx| {
                                            let mut items =
                                                SmallVec::with_capacity(range.end - range.start);
                                            this.iter_visible_entries(
                                                range,
                                                window,
                                                cx,
                                                &mut |entry, _, entries, _, _| {
                                                    let (depth, _) =
                                                        Self::calculate_depth_and_difference(
                                                            entry, entries,
                                                        );
                                                    items.push(depth);
                                                },
                                            );
                                            items
                                        },
                                    )
                                    .on_click(cx.listener(
                                        |this,
                                         active_indent_guide: &IndentGuideLayout,
                                         window,
                                         cx| {
                                            if window.modifiers().secondary() {
                                                let ix = active_indent_guide.offset.y;
                                                let Some((target_entry, worktree)) = maybe!({
                                                    let (worktree_id, entry) =
                                                        this.entry_at_index(ix)?;
                                                    let worktree = this
                                                        .project
                                                        .read(cx)
                                                        .worktree_for_id(worktree_id, cx)?;
                                                    let target_entry = worktree
                                                        .read(cx)
                                                        .entry_for_path(&entry.path.parent()?)?;
                                                    Some((target_entry, worktree))
                                                }) else {
                                                    return;
                                                };

                                                this.collapse_entry(
                                                    target_entry.clone(),
                                                    worktree,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    ))
                                    .with_render_fn(
                                        cx.entity(),
                                        move |this, params, _, cx| {
                                            const LEFT_OFFSET: Pixels = px(14.);
                                            const PADDING_Y: Pixels = px(4.);
                                            const HITBOX_OVERDRAW: Pixels = px(3.);

                                            let active_indent_guide_index = this
                                                .find_active_indent_guide(
                                                    &params.indent_guides,
                                                    cx,
                                                );

                                            let indent_size = params.indent_size;
                                            let item_height = params.item_height;

                                            params
                                                .indent_guides
                                                .into_iter()
                                                .enumerate()
                                                .map(|(idx, layout)| {
                                                    let offset = if layout.continues_offscreen {
                                                        px(0.)
                                                    } else {
                                                        PADDING_Y
                                                    };
                                                    let bounds = Bounds::new(
                                                        point(
                                                            layout.offset.x * indent_size
                                                                + LEFT_OFFSET,
                                                            layout.offset.y * item_height + offset,
                                                        ),
                                                        size(
                                                            px(1.),
                                                            layout.length * item_height
                                                                - offset * 2.,
                                                        ),
                                                    );
                                                    ui::RenderedIndentGuide {
                                                        bounds,
                                                        layout,
                                                        is_active: Some(idx)
                                                            == active_indent_guide_index,
                                                        hitbox: Some(Bounds::new(
                                                            point(
                                                                bounds.origin.x - HITBOX_OVERDRAW,
                                                                bounds.origin.y,
                                                            ),
                                                            size(
                                                                bounds.size.width
                                                                    + HITBOX_OVERDRAW * 2.,
                                                                bounds.size.height,
                                                            ),
                                                        )),
                                                    }
                                                })
                                                .collect()
                                        },
                                    ),
                                )
                            })
                            .when(show_sticky_entries, |list| {
                                let sticky_items = ui::sticky_items(
                                    cx.entity(),
                                    |this, range, window, cx| {
                                        let mut items =
                                            SmallVec::with_capacity(range.end - range.start);
                                        this.iter_visible_entries(
                                            range,
                                            window,
                                            cx,
                                            &mut |entry, index, entries, _, _| {
                                                let (depth, _) =
                                                    Self::calculate_depth_and_difference(
                                                        entry, entries,
                                                    );
                                                let candidate =
                                                    StickyProjectPanelCandidate { index, depth };
                                                items.push(candidate);
                                            },
                                        );
                                        items
                                    },
                                    |this, marker_entry, window, cx| {
                                        let sticky_entries =
                                            this.render_sticky_entries(marker_entry, window, cx);
                                        this.sticky_items_count = sticky_entries.len();
                                        sticky_entries
                                    },
                                );
                                list.with_decoration(if show_indent_guides {
                                    sticky_items.with_decoration(
                                        ui::indent_guides(
                                            px(indent_size),
                                            IndentGuideColors::panel(cx),
                                        )
                                        .with_render_fn(
                                            cx.entity(),
                                            move |_, params, _, _| {
                                                const LEFT_OFFSET: Pixels = px(14.);

                                                let indent_size = params.indent_size;
                                                let item_height = params.item_height;

                                                params
                                                    .indent_guides
                                                    .into_iter()
                                                    .map(|layout| {
                                                        let bounds = Bounds::new(
                                                            point(
                                                                layout.offset.x * indent_size
                                                                    + LEFT_OFFSET,
                                                                layout.offset.y * item_height,
                                                            ),
                                                            size(
                                                                px(1.),
                                                                layout.length * item_height,
                                                            ),
                                                        );
                                                        ui::RenderedIndentGuide {
                                                            bounds,
                                                            layout,
                                                            is_active: false,
                                                            hitbox: None,
                                                        }
                                                    })
                                                    .collect()
                                            },
                                        ),
                                    )
                                } else {
                                    sticky_items
                                })
                            })
                            .with_sizing_behavior(ListSizingBehavior::Infer)
                            .with_horizontal_sizing_behavior(if horizontal_scroll {
                                ListHorizontalSizingBehavior::Unconstrained
                            } else {
                                ListHorizontalSizingBehavior::FitList
                            })
                            .when(horizontal_scroll, |list| {
                                list.with_width_from_item(self.state.max_width_item_index)
                            })
                            .track_scroll(&self.scroll_handle),
                        )
                        .child(
                            div()
                                .id("project-panel-blank-area")
                                .block_mouse_except_scroll()
                                .flex_grow_1()
                                .on_scroll_wheel({
                                    let scroll_handle = self.scroll_handle.clone();
                                    let entity_id = cx.entity().entity_id();
                                    move |event, window, cx| {
                                        let state = scroll_handle.0.borrow();
                                        let base_handle = &state.base_handle;
                                        let current_offset = base_handle.offset();
                                        let max_offset = base_handle.max_offset();
                                        let delta = event.delta.pixel_delta(window.line_height());
                                        let new_offset = (current_offset + delta)
                                            .clamp(&max_offset.neg(), &Point::default());

                                        if new_offset != current_offset {
                                            base_handle.set_offset(new_offset);
                                            cx.notify(entity_id);
                                        }
                                    }
                                })
                                .when(
                                    self.drag_target_entry.as_ref().is_some_and(
                                        |entry| match entry {
                                            DragTarget::Background => true,
                                            DragTarget::Entry {
                                                highlight_entry_id, ..
                                            } => self.state.last_worktree_root_id.is_some_and(
                                                |root_id| *highlight_entry_id == root_id,
                                            ),
                                        },
                                    ),
                                    |div| div.bg(cx.theme().colors().drop_target_background),
                                )
                                .on_drag_move::<ExternalPaths>(cx.listener(
                                    move |this, event: &DragMoveEvent<ExternalPaths>, _, _| {
                                        let Some(_last_root_id) = this.state.last_worktree_root_id
                                        else {
                                            return;
                                        };
                                        if event.bounds.contains(&event.event.position) {
                                            this.drag_target_entry = Some(DragTarget::Background);
                                        } else {
                                            if this.drag_target_entry.as_ref().is_some_and(|e| {
                                                matches!(e, DragTarget::Background)
                                            }) {
                                                this.drag_target_entry = None;
                                            }
                                        }
                                    },
                                ))
                                .on_drag_move::<DraggedSelection>(cx.listener(
                                    move |this, event: &DragMoveEvent<DraggedSelection>, _, cx| {
                                        let Some(last_root_id) = this.state.last_worktree_root_id
                                        else {
                                            return;
                                        };
                                        if event.bounds.contains(&event.event.position) {
                                            let drag_state = event.drag(cx);
                                            if this.should_highlight_background_for_selection_drag(
                                                &drag_state,
                                                last_root_id,
                                                cx,
                                            ) {
                                                this.drag_target_entry =
                                                    Some(DragTarget::Background);
                                            }
                                        } else {
                                            if this.drag_target_entry.as_ref().is_some_and(|e| {
                                                matches!(e, DragTarget::Background)
                                            }) {
                                                this.drag_target_entry = None;
                                            }
                                        }
                                    },
                                ))
                                .on_drop(cx.listener(
                                    move |this, external_paths: &ExternalPaths, window, cx| {
                                        this.drag_target_entry = None;
                                        this.hover_scroll_task.take();
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.drop_external_files(
                                                external_paths.paths(),
                                                entry_id,
                                                window,
                                                cx,
                                            );
                                        }
                                        cx.stop_propagation();
                                    },
                                ))
                                .on_drop(cx.listener(
                                    move |this, selections: &DraggedSelection, window, cx| {
                                        this.drag_target_entry = None;
                                        this.hover_scroll_task.take();
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.drag_onto(selections, entry_id, false, window, cx);
                                        }
                                        cx.stop_propagation();
                                    },
                                ))
                                .on_click(cx.listener(|this, event, window, cx| {
                                    if matches!(event, gpui::ClickEvent::Keyboard(_)) {
                                        return;
                                    }
                                    cx.stop_propagation();
                                    this.selection = None;
                                    this.marked_entries.clear();
                                    this.focus_handle(cx).focus(window, cx);
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                        // When deploying the context menu anywhere below the last project entry,
                                        // act as if the user clicked the root of the last worktree.
                                        if let Some(entry_id) = this.state.last_worktree_root_id {
                                            this.deploy_context_menu(
                                                event.position,
                                                entry_id,
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                                )
                                .when(!project.is_read_only(cx), |el| {
                                    el.on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, window, cx| {
                                            if event.click_count() > 1
                                                && let Some(entry_id) =
                                                    this.state.last_worktree_root_id
                                            {
                                                let project = this.project.read(cx);

                                                let worktree_id = if let Some(worktree) =
                                                    project.worktree_for_entry(entry_id, cx)
                                                {
                                                    worktree.read(cx).id()
                                                } else {
                                                    return;
                                                };

                                                this.selection = Some(SelectedEntry {
                                                    worktree_id,
                                                    entry_id,
                                                });

                                                this.new_file(&NewFile, window, cx);
                                            }
                                        },
                                    ))
                                }),
                        )
                        .size_full(),
                )
                .custom_scrollbars(
                    {
                        let mut scrollbars =
                            Scrollbars::for_settings::<ProjectPanelScrollbarProxy>()
                                .tracked_scroll_handle(&self.scroll_handle);
                        if horizontal_scroll {
                            scrollbars = scrollbars.with_track_along(
                                ScrollAxes::Horizontal,
                                cx.theme().colors().editor_background,
                            );
                        }
                        scrollbars.notify_content()
                    },
                    window,
                    cx,
                )
                .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                    deferred(
                        anchored()
                            .position(*position)
                            .anchor(gpui::Anchor::TopLeft)
                            .child(menu.clone()),
                    )
                    .with_priority(3)
                }))
        } else {
            let focus_handle = self.focus_handle(cx);
            let workspace = self.workspace.clone();
            let workspace_clone = self.workspace.clone();

            v_flex()
                .id("empty-project_panel-wrapper")
                .size_full()
                .bg(cx.theme().colors().editor_background)
                .child(
                    ProjectEmptyState::new(
                        "Project Panel",
                        focus_handle.clone(),
                        KeyBinding::for_action_in(&workspace::Open::default(), &focus_handle, cx),
                    )
                    .on_open_project(move |_, window, cx| {
                        telemetry::event!("Project Panel Add Project Clicked");
                        workspace
                            .update(cx, |_, cx| {
                                window
                                    .dispatch_action(workspace::Open::default().boxed_clone(), cx);
                            })
                            .log_err();
                    })
                    .on_clone_repo(move |_, window, cx| {
                        telemetry::event!("Project Panel Clone Repo Clicked");
                        workspace_clone
                            .update(cx, |_, cx| {
                                window.dispatch_action(git::Clone.boxed_clone(), cx);
                            })
                            .log_err();
                    }),
                )
                .when(is_local, |div| {
                    div.when(panel_settings.drag_and_drop, |div| {
                        div.drag_over::<ExternalPaths>(|style, _, _, cx| {
                            style.bg(cx.theme().colors().drop_target_background)
                        })
                        .on_drop(cx.listener(
                            move |this, external_paths: &ExternalPaths, window, cx| {
                                this.drag_target_entry = None;
                                this.hover_scroll_task.take();
                                if let Some(task) = this
                                    .workspace
                                    .update(cx, |workspace, cx| {
                                        workspace.open_workspace_for_paths(
                                            OpenMode::Activate,
                                            external_paths.paths().to_owned(),
                                            window,
                                            cx,
                                        )
                                    })
                                    .log_err()
                                {
                                    task.detach_and_log_err(cx);
                                }
                                cx.stop_propagation();
                            },
                        ))
                    })
                })
        }
    }
}

#[cfg(test)]
mod project_panel_tests;
mod tests;
