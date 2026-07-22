#[cfg(test)]
mod file_finder_tests;

use futures::future::join_all;
pub use open_path_prompt::OpenPathDelegate;

use channel::ChannelStore;
use client::ChannelId;
use collections::HashMap;
use editor::Editor;
use file_icons::FileIcons;
use fuzzy::{StringMatch, StringMatchCandidate};
use fuzzy_nucleo::{PathMatch, PathMatchCandidate};
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Empty, Entity, EventEmitter, FocusHandle,
    Focusable, KeyContext, Modifiers, ModifiersChangedEvent, ParentElement, Render, Styled, Task,
    TaskExt, WeakEntity, Window, actions, rems,
};
use language::{BufferSnapshot, Point};
use mav_actions::search::ToggleIncludeIgnored;
use open_path_prompt::{
    OpenPathPrompt,
    file_finder_settings::{FileFinderSettings, FileFinderWidth},
};
use picker::{Picker, PickerDelegate};
use project::{
    PathMatchCandidateSet, Project, ProjectPath, WorktreeId, worktree_store::WorktreeStore,
};
use project_panel::project_panel_settings::ProjectPanelSettings;
use settings::Settings;
use std::{
    borrow::Cow,
    cmp,
    ops::{Range, RangeInclusive},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{self, AtomicBool},
    },
    time::Duration,
};
use ui::{HighlightedLabel, Indicator, ListItem, ListItemSpacing, Tooltip, prelude::*};
use util::{
    ResultExt, maybe,
    paths::{PathStyle, PathWithPosition},
    post_inc,
    rel_path::RelPath,
};
use workspace::{
    ModalView, OpenChannelNotesById, OpenOptions, OpenVisible, SplitDirection, Workspace,
    item::PreviewTabsSettings, notifications::NotifyResultExt, pane,
};

actions!(
    file_finder,
    [
        /// Selects the previous item in the file finder.
        SelectPrevious,
        /// Opens the selected file in the editor without dismissing the file finder,
        /// so additional files can be opened in sequence.
        OpenWithoutDismiss
    ]
);

impl ModalView for FileFinder {}

pub struct FileFinder {
    picker: Entity<Picker<FileFinderDelegate>>,
    picker_focus_handle: FocusHandle,
    init_modifiers: Option<Modifiers>,
}

pub fn init(cx: &mut App) {
    cx.observe_new(FileFinder::register).detach();
    cx.observe_new(OpenPathPrompt::register).detach();
    cx.observe_new(OpenPathPrompt::register_new_path).detach();
}

impl FileFinder {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _: &mut Context<Workspace>,
    ) {
        workspace.register_action(
            |workspace, action: &workspace::ToggleFileFinder, window, cx| {
                let Some(file_finder) = workspace.active_modal::<Self>(cx) else {
                    Self::open(workspace, action.separate_history, window, cx).detach();
                    return;
                };

                file_finder.update(cx, |file_finder, cx| {
                    file_finder.init_modifiers = Some(window.modifiers());
                    file_finder.picker.update(cx, |picker, cx| {
                        picker.cycle_selection(window, cx);
                    });
                });
            },
        );
    }

    fn open(
        workspace: &mut Workspace,
        separate_history: bool,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Task<()> {
        let project = workspace.project().read(cx);
        let fs = project.fs();

        let currently_opened_path = workspace.active_item(cx).and_then(|item| {
            let project_path = item.project_path(cx)?;
            let abs_path = project
                .worktree_for_id(project_path.worktree_id, cx)?
                .read(cx)
                .absolutize(&project_path.path);
            Some(FoundPath::new(project_path, abs_path))
        });

        let history_items = workspace
            .recent_navigation_history(Some(MAX_RECENT_SELECTIONS), cx)
            .into_iter()
            .filter_map(|(project_path, abs_path)| {
                if project.entry_for_path(&project_path, cx).is_some() {
                    return Some(Task::ready(Some(FoundPath::new(project_path, abs_path?))));
                }
                let abs_path = abs_path?;
                if project.is_local() {
                    let fs = fs.clone();
                    Some(cx.background_spawn(async move {
                        if fs.is_file(&abs_path).await {
                            Some(FoundPath::new(project_path, abs_path))
                        } else {
                            None
                        }
                    }))
                } else {
                    Some(Task::ready(Some(FoundPath::new(project_path, abs_path))))
                }
            })
            .collect::<Vec<_>>();
        cx.spawn_in(window, async move |workspace, cx| {
            let history_items = join_all(history_items).await.into_iter().flatten();

            workspace
                .update_in(cx, |workspace, window, cx| {
                    let project = workspace.project().clone();
                    let weak_workspace = cx.entity().downgrade();
                    workspace.toggle_modal(window, cx, |window, cx| {
                        let delegate = FileFinderDelegate::new(
                            cx.entity().downgrade(),
                            weak_workspace,
                            project,
                            currently_opened_path,
                            history_items.collect(),
                            separate_history,
                            window,
                            cx,
                        );

                        FileFinder::new(delegate, window, cx)
                    });
                })
                .ok();
        })
    }

    fn new(delegate: FileFinderDelegate, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let modal_max_width_setting = FileFinderSettings::get_global(cx).modal_max_width;

        let project = delegate.project.clone();
        let modal_max_width = Self::modal_max_width(modal_max_width_setting, window);
        let preview = picker_preview::editor_preview(project, window, cx);
        let picker = cx.new(|cx| {
            Picker::uniform_list_with_preview(delegate, preview, window, cx)
                .initial_width(Rems::from_pixels(modal_max_width, window))
        });
        let picker_focus_handle = picker.focus_handle(cx);
        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle.clone();
        });
        Self {
            picker,
            picker_focus_handle,
            init_modifiers: window.modifiers().modified().then_some(window.modifiers()),
        }
    }

    fn handle_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(init_modifiers) = self.init_modifiers.take() else {
            return;
        };
        if self.picker.read(cx).delegate.has_changed_selected_index
            && (!event.modified() || !init_modifiers.is_subset_of(event))
        {
            self.init_modifiers = None;
            window.dispatch_action(menu::Confirm.boxed_clone(), cx);
        }
    }

    fn handle_select_prev(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.init_modifiers = Some(window.modifiers());
        window.dispatch_action(Box::new(menu::SelectPrevious), cx);
    }

    fn handle_toggle_ignored(
        &mut self,
        _: &ToggleIncludeIgnored,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.include_ignored = match picker.delegate.include_ignored {
                Some(true) => FileFinderSettings::get_global(cx)
                    .include_ignored
                    .map(|_| false),
                Some(false) => Some(true),
                None => Some(true),
            };
            picker.delegate.include_ignored_refresh =
                picker.delegate.update_matches(picker.query(cx), window, cx);
        });
    }

    fn go_to_file_split_left(
        &mut self,
        _: &pane::SplitLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_file_split_inner(SplitDirection::Left, window, cx)
    }

    fn go_to_file_split_right(
        &mut self,
        _: &pane::SplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_file_split_inner(SplitDirection::Right, window, cx)
    }

    fn go_to_file_split_up(
        &mut self,
        _: &pane::SplitUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_file_split_inner(SplitDirection::Up, window, cx)
    }

    fn go_to_file_split_down(
        &mut self,
        _: &pane::SplitDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_file_split_inner(SplitDirection::Down, window, cx)
    }

    fn go_to_file_split_inner(
        &mut self,
        split_direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            let delegate = &mut picker.delegate;
            if let Some(workspace) = delegate.workspace.upgrade()
                && let Some(m) = delegate.matches.get(delegate.selected_index())
            {
                let path = match m {
                    Match::History { path, .. } => {
                        let worktree_id = path.project.worktree_id;
                        ProjectPath {
                            worktree_id,
                            path: Arc::clone(&path.project.path),
                        }
                    }
                    Match::Search(m) => project_path_for_search_match(&delegate.project, &m.0, cx),
                    Match::CreateNew(p) => p.clone(),
                    Match::Channel { .. } => return,
                };
                let open_task = workspace.update(cx, move |workspace, cx| {
                    workspace.split_path_preview(path, false, Some(split_direction), window, cx)
                });
                open_task.detach_and_log_err(cx);
            }
        })
    }

    fn open_without_dismiss(
        &mut self,
        _: &OpenWithoutDismiss,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.confirm_without_dismiss(window, cx);
        });
    }

    pub fn modal_max_width(width_setting: FileFinderWidth, window: &mut Window) -> Pixels {
        let window_width = window.viewport_size().width;
        let small_width = rems(34.).to_pixels(window.rem_size());

        match width_setting {
            FileFinderWidth::Small => small_width,
            FileFinderWidth::Full => window_width,
            FileFinderWidth::XLarge => (window_width - px(512.)).max(small_width),
            FileFinderWidth::Large => (window_width - px(768.)).max(small_width),
            FileFinderWidth::Medium => (window_width - px(1024.)).max(small_width),
        }
    }
}

impl EventEmitter<DismissEvent> for FileFinder {}

impl Focusable for FileFinder {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.picker_focus_handle.clone()
    }
}

impl Render for FileFinder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key_context = self.picker.read(cx).delegate.key_context(window, cx);

        v_flex()
            .key_context(key_context)
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_action(cx.listener(Self::handle_select_prev))
            .on_action(cx.listener(Self::handle_toggle_ignored))
            .on_action(cx.listener(Self::go_to_file_split_left))
            .on_action(cx.listener(Self::go_to_file_split_right))
            .on_action(cx.listener(Self::go_to_file_split_up))
            .on_action(cx.listener(Self::go_to_file_split_down))
            .on_action(cx.listener(Self::open_without_dismiss))
            .child(self.picker.clone())
    }
}

pub struct FileFinderDelegate {
    file_finder: WeakEntity<FileFinder>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    channel_store: Option<Entity<ChannelStore>>,
    search_count: usize,
    latest_search_id: usize,
    latest_search_did_cancel: bool,
    latest_search_query: Option<FileSearchQuery>,
    currently_opened_path: Option<FoundPath>,
    matches: Matches,
    selected_index: usize,
    has_changed_selected_index: bool,
    cancel_flag: Arc<AtomicBool>,
    search_in_flight: Arc<AtomicBool>,
    history_items: Vec<FoundPath>,
    separate_history: bool,
    first_update: bool,
    focus_handle: FocusHandle,
    include_ignored: Option<bool>,
    include_ignored_refresh: Task<()>,
}

const MAX_RECENT_SELECTIONS: usize = 20;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);

pub enum Event {
    Selected(ProjectPath),
    Dismissed,
}

const MAX_RECENT_SELECTIONS: usize = 20;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(100);

mod delegate_labels;
mod delegate_open;
mod delegate_search;
mod matches;
mod path_elision;
mod picker_delegate;
mod query;

use matches::{FoundPath, Match, Matches, ProjectPanelOrdMatch};
use path_elision::{PathComponentSlice, full_path_budget};
use query::{FileSearchQuery, parse_file_search_query};
