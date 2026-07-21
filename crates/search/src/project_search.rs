use crate::{
    BufferSearchBar, EXCLUDE_PLACEHOLDER, FocusSearch, HighlightKey, INCLUDE_PLACEHOLDER,
    NextHistoryQuery, PreviousHistoryQuery, REPLACE_PLACEHOLDER, ReplaceAll, ReplaceNext,
    SearchOption, SearchOptions, SearchSource, SelectNextMatch, SelectPreviousMatch,
    ToggleCaseSensitive, ToggleIncludeIgnored, ToggleRegex, ToggleReplace, ToggleWholeWord,
    buffer_search::Deploy,
    search_bar::{
        ActionButtonState, HistoryNavigationDirection, alignment_element, input_base_styles,
        render_action_button, render_text_input, should_navigate_history,
    },
    text_finder::TextFinder,
};
use anyhow::Context as _;
use collections::HashMap;
use editor::{
    Anchor, Editor, EditorEvent, EditorSettings, MAX_TAB_TITLE_LEN, MultiBuffer, PathKey,
    SelectionEffects,
    actions::{Backtab, FoldAll, SelectAll, Tab, UnfoldAll},
    items::active_match_index,
    multibuffer_context_lines,
    scroll::Autoscroll,
};
use futures::{StreamExt, stream::FuturesOrdered};
use gpui::{
    Action, AnyElement, App, AsyncApp, Axis, Context, Entity, EntityId, EventEmitter, FocusHandle,
    Focusable, Global, Hsla, InteractiveElement, IntoElement, KeyContext, ParentElement, Point,
    Render, SharedString, Styled, Subscription, Task, TaskExt, UpdateGlobal, WeakEntity, Window,
    actions, div,
};
use itertools::Itertools;
use language::{Buffer, Language};
use menu::Confirm;
use multi_buffer;
use project::{
    Project, ProjectPath, SearchResults,
    search::{SearchInputKind, SearchQuery, SearchResult},
    search_history::SearchHistoryCursor,
};
use settings::Settings;
use std::{
    any::{Any, TypeId},
    mem,
    ops::{Not, Range},
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use ui::{
    CommonAnimationExt, IconButtonShape, KeyBinding, Toggleable, Tooltip, prelude::*,
    utils::SearchInputWidth,
};
use util::{ResultExt as _, paths::PathMatcher, rel_path::RelPath};
use workspace::{
    DeploySearch, ItemNavHistory, NewSearch, ToolbarItemEvent, ToolbarItemLocation,
    ToolbarItemView, Workspace, WorkspaceId,
    item::{Item, ItemEvent, ItemHandle, SaveOptions},
    searchable::{Direction, SearchEvent, SearchToken, SearchableItem, SearchableItemHandle},
};

mod glob;
mod state;

use glob::split_glob_patterns;
use state::{InputPanel, ProjectSearchSettings, SearchActivity, SearchCompletion, SearchState};
pub use state::{ProjectSearch, ProjectSearchBar, ProjectSearchView};

actions!(
    project_search,
    [
        /// Searches in a new project search tab.
        SearchInNew,
        /// Toggles focus between the search bar and the search results.
        ToggleFocus,
        /// Moves to the next input field.
        NextField,
        /// Toggles the search filters panel.
        ToggleFilters,
        /// Toggles collapse/expand state of all search result excerpts.
        ToggleAllSearchResults,
        /// Open a text picker showing the current result in a modal.
        OpenTextFinder
    ]
);

#[derive(Default)]
pub(crate) struct ActiveSettings(pub(crate) HashMap<WeakEntity<Project>, ProjectSearchSettings>);

impl Global for ActiveSettings {}

pub fn init(cx: &mut App) {
    cx.set_global(ActiveSettings::default());
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        register_workspace_action(workspace, move |search_bar, _: &Deploy, window, cx| {
            search_bar.focus_search(window, cx);
        });
        register_workspace_action(workspace, move |search_bar, _: &FocusSearch, window, cx| {
            search_bar.focus_search(window, cx);
        });
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleFilters, window, cx| {
                search_bar.toggle_filters(window, cx);
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleCaseSensitive, window, cx| {
                search_bar.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx);
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, _: &ToggleWholeWord, window, cx| {
                search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
            },
        );
        register_workspace_action(workspace, move |search_bar, _: &ToggleRegex, window, cx| {
            search_bar.toggle_search_option(SearchOptions::REGEX, window, cx);
        });
        register_workspace_action(
            workspace,
            move |search_bar, action: &ToggleReplace, window, cx| {
                search_bar.toggle_replace(action, window, cx)
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, action: &SelectPreviousMatch, window, cx| {
                search_bar.select_prev_match(action, window, cx)
            },
        );
        register_workspace_action(
            workspace,
            move |search_bar, action: &SelectNextMatch, window, cx| {
                search_bar.select_next_match(action, window, cx)
            },
        );

        // Only handle search_in_new if there is a search present
        register_workspace_action_for_present_search(workspace, |workspace, action, window, cx| {
            ProjectSearchView::search_in_new(workspace, action, window, cx)
        });

        register_workspace_action_for_present_search(
            workspace,
            |workspace, action: &ToggleAllSearchResults, window, cx| {
                if let Some(search_view) = workspace
                    .active_item(cx)
                    .and_then(|item| item.downcast::<ProjectSearchView>())
                {
                    search_view.update(cx, |search_view, cx| {
                        search_view.toggle_all_search_results(action, window, cx);
                    });
                }
            },
        );

        register_workspace_action_for_present_search(
            workspace,
            |workspace, _: &menu::Cancel, window, cx| {
                if let Some(project_search_bar) = workspace
                    .active_pane()
                    .read(cx)
                    .toolbar()
                    .read(cx)
                    .item_of_type::<ProjectSearchBar>()
                {
                    project_search_bar.update(cx, |project_search_bar, cx| {
                        let search_is_focused = project_search_bar
                            .active_project_search
                            .as_ref()
                            .is_some_and(|search_view| {
                                search_view
                                    .read(cx)
                                    .query_editor
                                    .read(cx)
                                    .focus_handle(cx)
                                    .is_focused(window)
                            });
                        if search_is_focused {
                            project_search_bar.move_focus_to_results(window, cx);
                        } else {
                            project_search_bar.focus_search(window, cx)
                        }
                    });
                } else {
                    cx.propagate();
                }
            },
        );

        // Both on present and dismissed search, we need to unconditionally handle those actions to focus from the editor.
        workspace.register_action(move |workspace, action: &DeploySearch, window, cx| {
            if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
                cx.propagate();
                return;
            }
            ProjectSearchView::deploy_search(workspace, action, window, cx);
            cx.notify();
        });
        workspace.register_action(move |workspace, action: &NewSearch, window, cx| {
            if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
                cx.propagate();
                return;
            }
            ProjectSearchView::new_search(workspace, action, window, cx);
            cx.notify();
        });
    })
    .detach();
}

fn contains_uppercase(str: &str) -> bool {
    str.chars().any(|c| c.is_uppercase())
}

impl ProjectSearch {
    pub fn new(project: Entity<Project>, cx: &mut Context<Self>) -> Self {
        let capability = project.read(cx).capability();
        let excerpts = cx.new(|_| MultiBuffer::new(capability));
        let subscription = Self::subscribe_to_excerpts(&excerpts, cx);

        Self {
            project,
            excerpts,
            pending_search: Default::default(),
            match_ranges: Default::default(),
            active_query: None,
            last_search_query_text: None,
            search_id: 0,
            search_state: SearchState::Idle,
            search_history_cursor: Default::default(),
            search_included_history_cursor: Default::default(),
            search_excluded_history_cursor: Default::default(),
            project_search_turning_into_text_finder: Arc::new(AtomicBool::new(false)),
            _excerpts_subscription: subscription,
        }
    }

    fn clone(&self, cx: &mut Context<Self>) -> Entity<Self> {
        cx.new(|cx| {
            let excerpts = self
                .excerpts
                .update(cx, |excerpts, cx| cx.new(|cx| excerpts.clone(cx)));
            let subscription = Self::subscribe_to_excerpts(&excerpts, cx);

            Self {
                project: self.project.clone(),
                excerpts,
                pending_search: Default::default(),
                match_ranges: self.match_ranges.clone(),
                active_query: self.active_query.clone(),
                last_search_query_text: self.last_search_query_text.clone(),
                search_id: self.search_id,
                search_state: if self.pending_search.is_some() {
                    SearchState::Idle
                } else {
                    self.search_state
                },
                search_history_cursor: self.search_history_cursor.clone(),
                search_included_history_cursor: self.search_included_history_cursor.clone(),
                search_excluded_history_cursor: self.search_excluded_history_cursor.clone(),
                project_search_turning_into_text_finder: Arc::new(AtomicBool::new(false)),
                _excerpts_subscription: subscription,
            }
        })
    }
    fn subscribe_to_excerpts(
        excerpts: &Entity<MultiBuffer>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(excerpts, |this, _, event, cx| {
            if matches!(event, multi_buffer::Event::FileHandleChanged) {
                this.remove_deleted_buffers(cx);
            }
        })
    }

    fn remove_deleted_buffers(&mut self, cx: &mut Context<Self>) {
        let deleted_buffer_ids = self
            .excerpts
            .read(cx)
            .all_buffers_iter()
            .filter(|buffer| {
                buffer
                    .read(cx)
                    .file()
                    .is_some_and(|file| file.disk_state().is_deleted())
            })
            .map(|buffer| buffer.read(cx).remote_id())
            .collect::<Vec<_>>();

        if deleted_buffer_ids.is_empty() {
            return;
        }

        let snapshot = self.excerpts.update(cx, |excerpts, cx| {
            for buffer_id in deleted_buffer_ids {
                excerpts.remove_excerpts_for_buffer(buffer_id, cx);
            }
            excerpts.snapshot(cx)
        });

        self.match_ranges
            .retain(|range| snapshot.anchor_to_buffer_anchor(range.start).is_some());

        cx.notify();
    }

    fn cursor(&self, kind: SearchInputKind) -> &SearchHistoryCursor {
        match kind {
            SearchInputKind::Query => &self.search_history_cursor,
            SearchInputKind::Include => &self.search_included_history_cursor,
            SearchInputKind::Exclude => &self.search_excluded_history_cursor,
        }
    }
    fn cursor_mut(&mut self, kind: SearchInputKind) -> &mut SearchHistoryCursor {
        match kind {
            SearchInputKind::Query => &mut self.search_history_cursor,
            SearchInputKind::Include => &mut self.search_included_history_cursor,
            SearchInputKind::Exclude => &mut self.search_excluded_history_cursor,
        }
    }

    fn search(&mut self, query: SearchQuery, cx: &mut Context<Self>) {
        let project_search_turning_into_text_finder =
            Arc::clone(&self.project_search_turning_into_text_finder);
        let search = self.project.update(cx, |project, cx| {
            project
                .search_history_mut(SearchInputKind::Query)
                .add(&mut self.search_history_cursor, query.as_str().to_string());
            let included = query.as_inner().files_to_include().sources().join(",");
            if !included.is_empty() {
                project
                    .search_history_mut(SearchInputKind::Include)
                    .add(&mut self.search_included_history_cursor, included);
            }
            let excluded = query.as_inner().files_to_exclude().sources().join(",");
            if !excluded.is_empty() {
                project
                    .search_history_mut(SearchInputKind::Exclude)
                    .add(&mut self.search_excluded_history_cursor, excluded);
            }
            project.search(query.clone(), cx)
        });
        self.last_search_query_text = Some(query.as_str().to_string());
        self.search_id += 1;
        self.active_query = Some(query);
        self.match_ranges.clear();
        self.search_state = SearchState::Running(SearchActivity::Searching);
        self.pending_search = Some(cx.spawn(async move |project_search, cx| {
            project_search
                .update(cx, |project_search, cx| {
                    project_search.match_ranges.clear();
                    project_search
                        .excerpts
                        .update(cx, |excerpts, cx| excerpts.clear(cx));
                })
                .ok()?;

            consume_search_stream(
                project_search,
                search,
                project_search_turning_into_text_finder,
                cx,
            )
            .await
        }));
        cx.notify();
    }

    // At the point this is called the multibuffer has already been filled with
    // plundered results from the text finder
    pub(crate) fn hook_up_ongoing_search(
        &mut self,
        search_results: SearchResults<SearchResult>,
        cx: &mut Context<Self>,
    ) {
        let project_search_turning_into_text_finder =
            Arc::clone(&self.project_search_turning_into_text_finder);

        self.pending_search = Some(cx.spawn(async move |project_search, cx| {
            consume_search_stream(
                project_search,
                search_results,
                project_search_turning_into_text_finder,
                cx,
            )
            .await
        }));
        cx.notify();
    }
}

/// Drain a search result stream into the project search's multibuffer.
async fn consume_search_stream(
    project_search: WeakEntity<ProjectSearch>,
    search_results: SearchResults<SearchResult>,
    project_search_turning_into_text_finder: Arc<AtomicBool>,
    cx: &mut AsyncApp,
) -> Option<SearchResults<SearchResult>> {
    // Note: is cancel safe
    let mut matches = pin!(search_results.rx.clone().ready_chunks(1024));

    let mut limit_reached = false;
    while let Some(results) = matches.next().await {
        let (buffers_with_ranges, has_reached_limit, search_activity) = cx
            .background_executor()
            .spawn(async move {
                let mut limit_reached = false;
                let mut search_activity = None;
                let mut buffers_with_ranges = Vec::with_capacity(results.len());
                for result in results {
                    match result {
                        project::search::SearchResult::Buffer { buffer, ranges } => {
                            buffers_with_ranges.push((buffer, ranges));
                        }
                        project::search::SearchResult::LimitReached => {
                            limit_reached = true;
                        }
                        project::search::SearchResult::WaitingForScan => {
                            search_activity = Some(SearchActivity::WaitingForScan);
                        }
                        project::search::SearchResult::Searching => {
                            search_activity = Some(SearchActivity::Searching);
                        }
                    }
                }
                (buffers_with_ranges, limit_reached, search_activity)
            })
            .await;
        limit_reached |= has_reached_limit;
        if let Some(search_activity) = search_activity {
            project_search
                .update(cx, |project_search, cx| {
                    project_search.search_state = SearchState::Running(search_activity);
                    cx.notify();
                })
                .ok()?;
        }
        let mut new_ranges = project_search
            .update(cx, |project_search, cx| {
                project_search.excerpts.update(cx, |excerpts, cx| {
                    buffers_with_ranges
                        .into_iter()
                        .map(|(buffer, ranges)| {
                            excerpts.set_anchored_excerpts_for_path(
                                PathKey::for_buffer(&buffer, cx),
                                buffer,
                                ranges,
                                multibuffer_context_lines(cx),
                                cx,
                            )
                        })
                        .collect::<FuturesOrdered<_>>()
                })
            })
            .ok()?;
        while let Some(new_ranges) = new_ranges.next().await {
            // `new_ranges.next().await` likely never gets hit while still pending so `async_task`
            // will not reschedule, starving other front end tasks, insert a yield point for that here
            smol::future::yield_now().await;
            project_search
                .update(cx, |project_search, cx| {
                    project_search.match_ranges.extend(new_ranges);
                    cx.notify();
                })
                .ok()?;
        }

        // We do not want to end the task before all the results taken
        // from the mpsc rx are in
        if project_search_turning_into_text_finder.load(Ordering::Relaxed) {
            break;
        }
    }

    if project_search_turning_into_text_finder.load(Ordering::Relaxed) {
        project_search_turning_into_text_finder.store(false, Ordering::Relaxed); // reset
        return Some(search_results);
    }

    project_search
        .update(cx, |project_search, cx| {
            project_search.search_state = if project_search.match_ranges.is_empty() {
                SearchState::Completed(SearchCompletion::NoResults)
            } else {
                SearchState::Completed(SearchCompletion::Results { limit_reached })
            };
            project_search.pending_search.take();
            cx.notify();
        })
        .ok()?;

    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewEvent {
    UpdateTab,
    Activate,
    EditorEvent(editor::EditorEvent),
    Dismiss,
}

impl EventEmitter<ViewEvent> for ProjectSearchView {}

impl Render for ProjectSearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::default();
        key_context.add("ProjectSearchView");

        if self.has_matches() {
            div()
                .key_context(key_context)
                .on_action(cx.listener(Self::open_text_finder))
                .flex_1()
                .size_full()
                .track_focus(&self.focus_handle(cx))
                .child(self.results_editor.clone())
        } else {
            let model = self.entity.read(cx);

            let heading_text = match model.search_state {
                SearchState::Running(SearchActivity::WaitingForScan) => "Loading project…",
                SearchState::Running(SearchActivity::Searching) => "Searching…",
                SearchState::Completed(SearchCompletion::NoResults) => "No Results",
                _ => "Search All Files",
            };

            let heading_text = div()
                .justify_center()
                .child(Label::new(heading_text).size(LabelSize::Large));

            let page_content: Option<AnyElement> = match model.search_state {
                SearchState::Idle => Some(self.landing_text_minor(cx).into_any_element()),
                SearchState::Completed(SearchCompletion::NoResults) => Some(
                    Label::new("No results found in this project for the provided query")
                        .size(LabelSize::Small)
                        .into_any_element(),
                ),
                _ => None,
            };

            let page_content = page_content.map(|text| div().child(text));

            h_flex()
                .key_context(key_context)
                .on_action(cx.listener(Self::open_text_finder))
                .size_full()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .bg(cx.theme().colors().editor_background)
                .track_focus(&self.focus_handle(cx))
                .child(
                    v_flex()
                        .id("project-search-landing-page")
                        .overflow_y_scroll()
                        .gap_1()
                        .child(heading_text)
                        .children(page_content),
                )
        }
    }
}

impl Focusable for ProjectSearchView {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for ProjectSearchView {
    type Event = ViewEvent;
    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let query_text = self.query_editor.read(cx).text(cx);

        query_text
            .is_empty()
            .not()
            .then(|| query_text.into())
            .or_else(|| Some("Project Search".into()))
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            Some(self.results_editor.clone().into())
        } else {
            None
        }
    }
    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.results_editor.clone()))
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.results_editor
            .update(cx, |editor, cx| editor.deactivated(window, cx));
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::MagnifyingGlass))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        let last_query: Option<SharedString> = self
            .entity
            .read(cx)
            .last_search_query_text
            .as_ref()
            .map(|query| {
                let query = query.replace('\n', "");
                let query_text = util::truncate_and_trailoff(&query, MAX_TAB_TITLE_LEN);
                query_text.into()
            });

        last_query
            .filter(|query| !query.is_empty())
            .unwrap_or_else(|| "Project Search".into())
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("Project Search Opened")
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        self.results_editor.for_each_project_item(cx, f)
    }

    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.results_editor.read(cx).active_project_path(cx)
    }

    fn can_save(&self, _: &App) -> bool {
        true
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.results_editor.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.results_editor.read(cx).has_conflict(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.results_editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        _: Entity<Project>,
        _: ProjectPath,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        unreachable!("save_as should not have been called")
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.results_editor
            .update(cx, |editor, cx| editor.reload(project, window, cx))
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        let model = self.entity.update(cx, |model, cx| model.clone(cx));
        Task::ready(Some(cx.new(|cx| {
            Self::new(self.workspace.clone(), model, window, cx, None)
        })))
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.results_editor.update(cx, |editor, cx| {
            editor.added_to_workspace(workspace, window, cx)
        });
    }

    fn set_nav_history(
        &mut self,
        nav_history: ItemNavHistory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.results_editor.update(cx, |editor, _| {
            editor.set_nav_history(Some(nav_history));
        });
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.results_editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            ViewEvent::UpdateTab => {
                f(ItemEvent::UpdateBreadcrumbs);
                f(ItemEvent::UpdateTab);
            }
            ViewEvent::EditorEvent(editor_event) => {
                Editor::to_item_events(editor_event, f);
            }
            ViewEvent::Dismiss => f(ItemEvent::CloseItem),
            _ => {}
        }
    }
}

impl ProjectSearchView {
    pub fn get_matches(&self, cx: &App) -> Vec<Range<Anchor>> {
        self.entity.read(cx).match_ranges.clone()
    }

    fn open_text_finder(
        &mut self,
        _: &OpenTextFinder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        TextFinder::open_from_project_search(cx.entity(), window, cx).detach();
    }

    fn toggle_filters(&mut self, cx: &mut Context<Self>) {
        self.filters_enabled = !self.filters_enabled;
        ActiveSettings::update_global(cx, |settings, cx| {
            settings.0.insert(
                self.entity.read(cx).project.downgrade(),
                self.current_settings(),
            );
        });
    }

    fn current_settings(&self) -> ProjectSearchSettings {
        ProjectSearchSettings {
            search_options: self.search_options,
            filters_enabled: self.filters_enabled,
        }
    }

    fn set_search_option_enabled(
        &mut self,
        option: SearchOptions,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.search_options.contains(option) != enabled {
            self.toggle_search_option(option, cx);
        }
    }

    fn toggle_search_option(&mut self, option: SearchOptions, cx: &mut Context<Self>) {
        self.search_options.toggle(option);
        ActiveSettings::update_global(cx, |settings, cx| {
            settings.0.insert(
                self.entity.read(cx).project.downgrade(),
                self.current_settings(),
            );
        });
        self.adjust_query_regex_language(cx);
    }

    fn toggle_opened_only(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.included_opened_only = !self.included_opened_only;
    }

    pub fn replacement(&self, cx: &App) -> String {
        self.replacement_editor.read(cx).text(cx)
    }

    fn replace_next(&mut self, _: &ReplaceNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.entity.read(cx).pending_search.is_some() {
            return;
        }
        if let Some(last_search_query_text) = &self.entity.read(cx).last_search_query_text
            && self.query_editor.read(cx).text(cx) != *last_search_query_text
        {
            // search query has changed, restart search and bail
            self.search(cx);
            return;
        }
        if self.entity.read(cx).match_ranges.is_empty() {
            return;
        }
        let Some(active_index) = self.active_match_index else {
            return;
        };

        let query = self.entity.read(cx).active_query.clone();
        if let Some(query) = query {
            let query = query.with_replacement(self.replacement(cx));

            let mat = self.entity.read(cx).match_ranges.get(active_index).cloned();
            self.results_editor.update(cx, |editor, cx| {
                if let Some(mat) = mat.as_ref() {
                    editor.replace(mat, &query, SearchToken::default(), window, cx);
                }
            });
            self.select_match(Direction::Next, window, cx)
        }
    }

    fn replace_all(&mut self, _: &ReplaceAll, window: &mut Window, cx: &mut Context<Self>) {
        if self.entity.read(cx).pending_search.is_some() {
            self.pending_replace_all = true;
            return;
        }
        let query_text = self.query_editor.read(cx).text(cx);
        let query_is_stale =
            self.entity.read(cx).last_search_query_text.as_deref() != Some(query_text.as_str());
        if query_is_stale {
            self.pending_replace_all = true;
            self.search(cx);
            if self.entity.read(cx).pending_search.is_none() {
                self.pending_replace_all = false;
            }
            return;
        }
        self.pending_replace_all = false;
        if self.active_match_index.is_none() {
            return;
        }
        let Some(query) = self.entity.read(cx).active_query.as_ref() else {
            return;
        };
        let query = query.clone().with_replacement(self.replacement(cx));

        let match_ranges = self
            .entity
            .update(cx, |model, _| mem::take(&mut model.match_ranges));
        if match_ranges.is_empty() {
            return;
        }

        self.results_editor.update(cx, |editor, cx| {
            editor.replace_all(
                &mut match_ranges.iter(),
                &query,
                SearchToken::default(),
                window,
                cx,
            );
        });

        self.entity.update(cx, |model, _cx| {
            model.match_ranges = match_ranges;
        });
    }

    fn toggle_all_search_results(
        &mut self,
        _: &ToggleAllSearchResults,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_results_visibility(window, cx);
    }

    fn update_results_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_any_folded = self.results_editor.read(cx).has_any_buffer_folded(cx);
        self.results_editor.update(cx, |editor, cx| {
            if has_any_folded {
                editor.unfold_all(&UnfoldAll, window, cx);
            } else {
                editor.fold_all(&FoldAll, window, cx);
            }
        });
        cx.notify();
    }

    pub fn new(
        workspace: WeakEntity<Workspace>,
        entity: Entity<ProjectSearch>,
        window: &mut Window,
        cx: &mut Context<Self>,
        settings: Option<ProjectSearchSettings>,
    ) -> Self {
        let project;
        let excerpts;
        let mut replacement_text = None;
        let mut query_text = String::new();
        let mut subscriptions = Vec::new();

        // Read in settings if available
        let (mut options, filters_enabled) = if let Some(settings) = settings {
            (settings.search_options, settings.filters_enabled)
        } else {
            let search_options =
                SearchOptions::from_settings(&EditorSettings::get_global(cx).search);
            (search_options, false)
        };

        {
            let entity = entity.read(cx);
            project = entity.project.clone();
            excerpts = entity.excerpts.clone();
            if let Some(active_query) = entity.active_query.as_ref() {
                query_text = active_query.as_str().to_string();
                replacement_text = active_query.replacement().map(ToOwned::to_owned);
                options = SearchOptions::from_query(active_query);
            }
        }
        subscriptions.push(cx.observe_in(&entity, window, |this, _, window, cx| {
            this.entity_changed(window, cx)
        }));

        let query_editor = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, 4, window, cx);
            editor.set_placeholder_text("Search all files…", window, cx);
            editor.set_use_autoclose(false);
            editor.set_use_selection_highlight(false);
            editor.set_text(query_text, window, cx);
            editor
        });
        // Subscribe to query_editor in order to reraise editor events for workspace item activation purposes
        subscriptions.push(
            cx.subscribe(&query_editor, |this, _, event: &EditorEvent, cx| {
                if let EditorEvent::Edited { .. } = event
                    && EditorSettings::get_global(cx).use_smartcase_search
                {
                    let query = this.search_query_text(cx);
                    if !query.is_empty()
                        && this.search_options.contains(SearchOptions::CASE_SENSITIVE)
                            != contains_uppercase(&query)
                    {
                        this.toggle_search_option(SearchOptions::CASE_SENSITIVE, cx);
                    }
                }
                cx.emit(ViewEvent::EditorEvent(event.clone()))
            }),
        );
        let replacement_editor = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, 4, window, cx);
            editor.set_placeholder_text(REPLACE_PLACEHOLDER, window, cx);
            if let Some(text) = replacement_text {
                editor.set_text(text, window, cx);
            }
            editor
        });
        let results_editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(excerpts, Some(project.clone()), window, cx);
            editor.set_searchable(false);
            editor.set_in_project_search(true);
            editor
        });
        subscriptions.push(cx.observe(&results_editor, |_, _, cx| cx.emit(ViewEvent::UpdateTab)));

        subscriptions.push(
            cx.subscribe(&results_editor, |this, _, event: &EditorEvent, cx| {
                if matches!(event, editor::EditorEvent::SelectionsChanged { .. }) {
                    this.update_match_index(cx);
                }
                // Reraise editor events for workspace item activation purposes
                cx.emit(ViewEvent::EditorEvent(event.clone()));
            }),
        );
        subscriptions.push(cx.subscribe(
            &results_editor,
            |_this, _editor, _event: &SearchEvent, cx| cx.notify(),
        ));

        let included_files_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(INCLUDE_PLACEHOLDER, window, cx);

            editor
        });
        // Subscribe to include_files_editor in order to reraise editor events for workspace item activation purposes
        subscriptions.push(
            cx.subscribe(&included_files_editor, |_, _, event: &EditorEvent, cx| {
                cx.emit(ViewEvent::EditorEvent(event.clone()))
            }),
        );

        let excluded_files_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(EXCLUDE_PLACEHOLDER, window, cx);

            editor
        });
        // Subscribe to excluded_files_editor in order to reraise editor events for workspace item activation purposes
        subscriptions.push(
            cx.subscribe(&excluded_files_editor, |_, _, event: &EditorEvent, cx| {
                cx.emit(ViewEvent::EditorEvent(event.clone()))
            }),
        );

        let focus_handle = cx.focus_handle();
        subscriptions.push(cx.on_focus(&focus_handle, window, |_, window, cx| {
            cx.on_next_frame(window, |this, window, cx| {
                if this.focus_handle.is_focused(window) {
                    if this.has_matches() {
                        this.results_editor.focus_handle(cx).focus(window, cx);
                    } else {
                        this.query_editor.focus_handle(cx).focus(window, cx);
                    }
                }
            });
        }));

        let languages = project.read(cx).languages().clone();
        cx.spawn(async move |project_search_view, cx| {
            let regex_language = languages
                .language_for_name("regex")
                .await
                .context("loading regex language")?;
            project_search_view
                .update(cx, |project_search_view, cx| {
                    project_search_view.regex_language = Some(regex_language);
                    project_search_view.adjust_query_regex_language(cx);
                })
                .ok();
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        // Check if Worktrees have all been previously indexed
        let mut this = ProjectSearchView {
            workspace,
            focus_handle,
            replacement_editor,
            search_id: entity.read(cx).search_id,
            entity,
            query_editor,
            results_editor,
            search_options: options,
            panels_with_errors: HashMap::default(),
            active_match_index: None,
            included_files_editor,
            excluded_files_editor,
            filters_enabled,
            replace_enabled: false,
            pending_replace_all: false,
            included_opened_only: false,
            regex_language: None,
            _subscriptions: subscriptions,
        };

        this.entity_changed(window, cx);
        this
    }

    pub fn new_search_in_directory(
        workspace: &mut Workspace,
        dir_path: &RelPath,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let filter_str = dir_path.display(workspace.path_style(cx));

        let weak_workspace = cx.entity().downgrade();

        let entity = cx.new(|cx| ProjectSearch::new(workspace.project().clone(), cx));
        let search = cx.new(|cx| ProjectSearchView::new(weak_workspace, entity, window, cx, None));
        workspace.add_item_to_active_pane(Box::new(search.clone()), None, true, window, cx);
        search.update(cx, |search, cx| {
            search
                .included_files_editor
                .update(cx, |editor, cx| editor.set_text(filter_str, window, cx));
            search.filters_enabled = true;
            search.focus_query_editor(window, cx)
        });
    }

    /// Re-activate the most recently activated search in this pane or the most recent if it has been closed.
    /// If no search exists in the workspace, create a new one.
    pub fn deploy_search(
        workspace: &mut Workspace,
        action: &workspace::DeploySearch,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let existing = workspace
            .active_pane()
            .read(cx)
            .items()
            .find_map(|item| item.downcast::<ProjectSearchView>());

        Self::existing_or_new_search(workspace, existing, action, window, cx);
    }

    fn search_in_new(
        workspace: &mut Workspace,
        _: &SearchInNew,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if let Some(search_view) = workspace
            .active_item(cx)
            .and_then(|item| item.downcast::<ProjectSearchView>())
        {
            let new_query = search_view.update(cx, |search_view, cx| {
                let open_buffers = if search_view.included_opened_only {
                    Some(search_view.open_buffers(cx, workspace))
                } else {
                    None
                };
                let new_query = search_view.build_search_query(cx, open_buffers);
                if new_query.is_some()
                    && let Some(old_query) = search_view.entity.read(cx).active_query.clone()
                {
                    search_view.query_editor.update(cx, |editor, cx| {
                        editor.set_text(old_query.as_str(), window, cx);
                    });
                    search_view.search_options = SearchOptions::from_query(&old_query);
                    search_view.adjust_query_regex_language(cx);
                }
                new_query
            });
            if let Some(new_query) = new_query {
                let entity = cx.new(|cx| {
                    let mut entity = ProjectSearch::new(workspace.project().clone(), cx);
                    entity.search(new_query, cx);
                    entity
                });
                let weak_workspace = cx.entity().downgrade();
                workspace.add_item_to_active_pane(
                    Box::new(cx.new(|cx| {
                        ProjectSearchView::new(weak_workspace, entity, window, cx, None)
                    })),
                    None,
                    true,
                    window,
                    cx,
                );
            }
        }
    }

    // Add another search tab to the workspace.
    fn new_search(
        workspace: &mut Workspace,
        _: &workspace::NewSearch,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        Self::existing_or_new_search(workspace, None, &DeploySearch::default(), window, cx)
    }

    fn existing_or_new_search(
        workspace: &mut Workspace,
        existing: Option<Entity<ProjectSearchView>>,
        action: &workspace::DeploySearch,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let query = workspace.active_item(cx).and_then(|item| {
            if let Some(buffer_search_query) = buffer_search_query(workspace, item.as_ref(), cx) {
                return Some(buffer_search_query);
            }

            let editor = item.act_as::<Editor>(cx)?;
            let query = editor.query_suggestion(None, window, cx);
            if query.is_empty() { None } else { Some(query) }
        });

        let search = if let Some(existing) = existing {
            workspace.activate_item(&existing, true, true, window, cx);
            existing
        } else {
            let settings = cx
                .global::<ActiveSettings>()
                .0
                .get(&workspace.project().downgrade());

            let settings = settings.cloned();

            let weak_workspace = cx.entity().downgrade();

            let project_search = cx.new(|cx| ProjectSearch::new(workspace.project().clone(), cx));
            let project_search_view = cx.new(|cx| {
                ProjectSearchView::new(weak_workspace, project_search, window, cx, settings)
            });

            workspace.add_item_to_active_pane(
                Box::new(project_search_view.clone()),
                None,
                true,
                window,
                cx,
            );
            project_search_view
        };

        search.update(cx, |search, cx| {
            search.replace_enabled |= action.replace_enabled;
            if let Some(regex) = action.regex {
                search.set_search_option_enabled(SearchOptions::REGEX, regex, cx);
            }
            if let Some(case_sensitive) = action.case_sensitive {
                search.set_search_option_enabled(SearchOptions::CASE_SENSITIVE, case_sensitive, cx);
            }
            if let Some(whole_word) = action.whole_word {
                search.set_search_option_enabled(SearchOptions::WHOLE_WORD, whole_word, cx);
            }
            if let Some(include_ignored) = action.include_ignored {
                search.set_search_option_enabled(
                    SearchOptions::INCLUDE_IGNORED,
                    include_ignored,
                    cx,
                );
            }
            let query = action
                .query
                .as_deref()
                .filter(|q| !q.is_empty())
                .or(query.as_deref());
            if let Some(query) = query {
                search.set_query(query, window, cx);
            }
            if let Some(included_files) = action.included_files.as_deref() {
                search
                    .included_files_editor
                    .update(cx, |editor, cx| editor.set_text(included_files, window, cx));
                search.filters_enabled = true;
            }
            if let Some(excluded_files) = action.excluded_files.as_deref() {
                search
                    .excluded_files_editor
                    .update(cx, |editor, cx| editor.set_text(excluded_files, window, cx));
                search.filters_enabled = true;
            }
            search.focus_query_editor(window, cx)
        });
    }

    fn prompt_to_save_if_dirty_then_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let project = self.entity.read(cx).project.clone();

        let can_autosave = self.results_editor.can_autosave(cx);
        let autosave_setting = self.results_editor.workspace_settings(cx).autosave;

        let will_autosave = can_autosave && autosave_setting.should_save_on_close();

        let is_dirty = self.is_dirty(cx);

        cx.spawn_in(window, async move |this, cx| {
            let skip_save_on_close = this
                .read_with(cx, |this, cx| {
                    this.workspace.read_with(cx, |workspace, cx| {
                        workspace::Pane::skip_save_on_close(&this.results_editor, workspace, cx)
                    })
                })?
                .unwrap_or(false);

            let should_prompt_to_save = !skip_save_on_close && !will_autosave && is_dirty;

            let should_search = if should_prompt_to_save {
                let options = &["Save", "Don't Save", "Cancel"];
                let result_channel = this.update_in(cx, |_, window, cx| {
                    window.prompt(
                        gpui::PromptLevel::Warning,
                        "Project search buffer contains unsaved edits. Do you want to save it?",
                        None,
                        options,
                        cx,
                    )
                })?;
                let result = result_channel.await?;
                let should_save = result == 0;
                if should_save {
                    this.update_in(cx, |this, window, cx| {
                        this.save(
                            SaveOptions {
                                format: true,
                                force_format: false,
                                autosave: false,
                            },
                            project,
                            window,
                            cx,
                        )
                    })?
                    .await
                    .log_err();
                }

                result != 2
            } else {
                true
            };
            if should_search {
                this.update(cx, |this, cx| {
                    this.search(cx);
                })?;
            }
            anyhow::Ok(())
        })
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let open_buffers = if self.included_opened_only {
            self.workspace
                .update(cx, |workspace, cx| self.open_buffers(cx, workspace))
                .ok()
        } else {
            None
        };
        if let Some(query) = self.build_search_query(cx, open_buffers) {
            self.entity.update(cx, |model, cx| model.search(query, cx));
        }
    }

    pub fn search_query_text(&self, cx: &App) -> String {
        self.query_editor.read(cx).text(cx)
    }

    fn build_search_query(
        &mut self,
        cx: &mut Context<Self>,
        open_buffers: Option<Vec<Entity<Buffer>>>,
    ) -> Option<SearchQuery> {
        // Do not bail early in this function, as we want to fill out `self.panels_with_errors`.

        let text = self.search_query_text(cx);
        let included_files = self
            .filters_enabled
            .then(|| {
                match self.parse_path_matches(self.included_files_editor.read(cx).text(cx), cx) {
                    Ok(included_files) => {
                        let should_unmark_error =
                            self.panels_with_errors.remove(&InputPanel::Include);
                        if should_unmark_error.is_some() {
                            cx.notify();
                        }
                        included_files
                    }
                    Err(e) => {
                        let should_mark_error = self
                            .panels_with_errors
                            .insert(InputPanel::Include, e.to_string());
                        if should_mark_error.is_none() {
                            cx.notify();
                        }
                        PathMatcher::default()
                    }
                }
            })
            .unwrap_or(PathMatcher::default());
        let excluded_files = self
            .filters_enabled
            .then(|| {
                match self.parse_path_matches(self.excluded_files_editor.read(cx).text(cx), cx) {
                    Ok(excluded_files) => {
                        let should_unmark_error =
                            self.panels_with_errors.remove(&InputPanel::Exclude);
                        if should_unmark_error.is_some() {
                            cx.notify();
                        }

                        excluded_files
                    }
                    Err(e) => {
                        let should_mark_error = self
                            .panels_with_errors
                            .insert(InputPanel::Exclude, e.to_string());
                        if should_mark_error.is_none() {
                            cx.notify();
                        }
                        PathMatcher::default()
                    }
                }
            })
            .unwrap_or(PathMatcher::default());

        // If the project contains multiple visible worktrees, we match the
        // include/exclude patterns against full paths to allow them to be
        // disambiguated. For single worktree projects we use worktree relative
        // paths for convenience.
        let match_full_paths = self
            .entity
            .read(cx)
            .project
            .read(cx)
            .visible_worktrees(cx)
            .count()
            > 1;

        let query = match self.search_options.build_query(
            text,
            included_files,
            excluded_files,
            match_full_paths,
            open_buffers,
        ) {
            Ok(query) => {
                let should_unmark_error = self.panels_with_errors.remove(&InputPanel::Query);
                if should_unmark_error.is_some() {
                    cx.notify();
                }

                Some(query)
            }
            Err(e) => {
                let should_mark_error = self
                    .panels_with_errors
                    .insert(InputPanel::Query, e.to_string());
                if should_mark_error.is_none() {
                    cx.notify();
                }

                None
            }
        };
        if !self.panels_with_errors.is_empty() {
            return None;
        }
        if query.as_ref().is_some_and(|query| query.is_empty()) {
            return None;
        }
        query
    }

    fn open_buffers(&self, cx: &App, workspace: &Workspace) -> Vec<Entity<Buffer>> {
        let mut buffers = Vec::new();
        for editor in workspace.items_of_type::<Editor>(cx) {
            if let Some(buffer) = editor.read(cx).buffer().read(cx).as_singleton() {
                buffers.push(buffer);
            }
        }
        buffers
    }

    /// The include/exclude path matchers currently configured on this view,
    /// honoring `filters_enabled`. Read-only (unlike `build_search_query` it does
    /// not record parse errors in `panels_with_errors`); invalid globs fall back
    /// to a default (match-all) matcher. Shared with the text finder, which is
    /// backed by the same view.
    pub(crate) fn file_path_filters(&self, cx: &App) -> (PathMatcher, PathMatcher) {
        if !self.filters_enabled {
            return (PathMatcher::default(), PathMatcher::default());
        }
        let included = self
            .parse_path_matches(self.included_files_editor.read(cx).text(cx), cx)
            .unwrap_or_default();
        let excluded = self
            .parse_path_matches(self.excluded_files_editor.read(cx).text(cx), cx)
            .unwrap_or_default();
        (included, excluded)
    }

    fn parse_path_matches(&self, text: String, cx: &App) -> anyhow::Result<PathMatcher> {
        let path_style = self.entity.read(cx).project.read(cx).path_style(cx);
        let queries = split_glob_patterns(&text)
            .into_iter()
            .map(str::trim)
            .filter(|maybe_glob_str| !maybe_glob_str.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        Ok(PathMatcher::new(&queries, path_style)?)
    }

    fn select_match(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.active_match_index {
            let match_ranges = self.entity.read(cx).match_ranges.clone();

            if !EditorSettings::get_global(cx).search_wrap
                && ((direction == Direction::Next && index + 1 >= match_ranges.len())
                    || (direction == Direction::Prev && index == 0))
            {
                crate::show_no_more_matches(window, cx);
                return;
            }

            let new_index = self.results_editor.update(cx, |editor, cx| {
                editor.match_index_for_direction(
                    &match_ranges,
                    index,
                    direction,
                    1,
                    SearchToken::default(),
                    window,
                    cx,
                )
            });

            let range_to_select = match_ranges[new_index].clone();
            self.results_editor.update(cx, |editor, cx| {
                let range_to_select = editor.range_for_match(&range_to_select);
                let autoscroll = if EditorSettings::get_global(cx).search.center_on_match {
                    Autoscroll::center()
                } else {
                    Autoscroll::fit()
                };
                editor.unfold_ranges(std::slice::from_ref(&range_to_select), false, true, cx);
                editor.change_selections(SelectionEffects::scroll(autoscroll), window, cx, |s| {
                    s.select_ranges([range_to_select])
                });
            });
            self.highlight_matches(&match_ranges, Some(new_index), cx);
        }
    }

    fn focus_query_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_editor.update(cx, |query_editor, cx| {
            query_editor.select_all(&SelectAll, window, cx);
        });
        let editor_handle = self.query_editor.focus_handle(cx);
        window.focus(&editor_handle, cx);
    }

    /// Apply some state (from the textfinder) to the project search UI
    pub(crate) fn adopt_text_finder_state(
        &mut self,
        search_options: SearchOptions,
        active_query: Option<SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_options = search_options;
        self.adjust_query_regex_language(cx);
        if let Some(query) = active_query {
            let query_text = query.as_str().to_string();
            self.entity.update(cx, |search, _| {
                search.active_query = Some(query.clone());
                search.last_search_query_text = Some(query_text.clone());
                // Force `entity_changed` to treat this as a new search so the
                // first match gets selected and scrolled into view. The text
                // finder ran its searches via `project.search` directly, so the
                // entity's `search_id` was never advanced.
                search.search_id += 1;
            });
            self.set_search_editor(SearchInputKind::Query, &query_text, window, cx);
            self.focus_results_editor(window, cx);
        } else {
            self.focus_query_editor(window, cx);
        }
        self.entity_changed(window, cx);
    }

    fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.set_search_editor(SearchInputKind::Query, query, window, cx);
        if EditorSettings::get_global(cx).use_smartcase_search
            && !query.is_empty()
            && self.search_options.contains(SearchOptions::CASE_SENSITIVE)
                != contains_uppercase(query)
        {
            self.toggle_search_option(SearchOptions::CASE_SENSITIVE, cx)
        }
    }

    fn set_search_editor(
        &mut self,
        kind: SearchInputKind,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = match kind {
            SearchInputKind::Query => &self.query_editor,
            SearchInputKind::Include => &self.included_files_editor,

            SearchInputKind::Exclude => &self.excluded_files_editor,
        };
        editor.update(cx, |editor, cx| {
            editor.set_text(text, window, cx);
            editor.request_autoscroll(Autoscroll::fit(), cx);
        });
    }

    fn focus_results_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_editor.update(cx, |query_editor, cx| {
            let cursor = query_editor.selections.newest_anchor().head();
            query_editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([cursor..cursor])
            });
        });
        let results_handle = self.results_editor.focus_handle(cx);
        window.focus(&results_handle, cx);
    }

    fn entity_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let match_ranges = self.entity.read(cx).match_ranges.clone();

        if match_ranges.is_empty() {
            self.active_match_index = None;
            self.results_editor.update(cx, |editor, cx| {
                editor.clear_background_highlights(HighlightKey::ProjectSearchView, cx);
            });
        } else {
            self.active_match_index = Some(0);
            self.update_match_index(cx);
            let prev_search_id = mem::replace(&mut self.search_id, self.entity.read(cx).search_id);
            let is_new_search = self.search_id != prev_search_id;
            self.results_editor.update(cx, |editor, cx| {
                if is_new_search {
                    let range_to_select = match_ranges
                        .first()
                        .map(|range| editor.range_for_match(range));
                    editor.change_selections(Default::default(), window, cx, |s| {
                        s.select_ranges(range_to_select)
                    });
                    editor.scroll(Point::default(), Some(Axis::Vertical), window, cx);
                }
            });
            if is_new_search && self.query_editor.focus_handle(cx).is_focused(window) {
                self.focus_results_editor(window, cx);
            }
        }

        cx.emit(ViewEvent::UpdateTab);
        cx.notify();

        if self.pending_replace_all && self.entity.read(cx).pending_search.is_none() {
            self.replace_all(&ReplaceAll, window, cx);
        }
    }

    fn update_match_index(&mut self, cx: &mut Context<Self>) {
        let results_editor = self.results_editor.read(cx);
        let newest_anchor = results_editor.selections.newest_anchor().head();
        let buffer_snapshot = results_editor.buffer().read(cx).snapshot(cx);
        let new_index = self.entity.update(cx, |this, cx| {
            let new_index = active_match_index(
                Direction::Next,
                &this.match_ranges,
                &newest_anchor,
                &buffer_snapshot,
            );

            self.highlight_matches(&this.match_ranges, new_index, cx);
            new_index
        });

        if self.active_match_index != new_index {
            self.active_match_index = new_index;
            cx.notify();
        }
    }

    #[ztracing::instrument(skip_all)]
    fn highlight_matches(
        &self,
        match_ranges: &[Range<Anchor>],
        active_index: Option<usize>,
        cx: &mut App,
    ) {
        self.results_editor.update(cx, |editor, cx| {
            editor.highlight_background(
                HighlightKey::ProjectSearchView,
                match_ranges,
                move |index, theme| {
                    if active_index == Some(*index) {
                        theme.colors().search_active_match_background
                    } else {
                        theme.colors().search_match_background
                    }
                },
                cx,
            );
        });
    }

    pub fn has_matches(&self) -> bool {
        self.active_match_index.is_some()
    }

    fn landing_text_minor(&self, cx: &App) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        v_flex()
            .gap_1()
            .child(
                Label::new("Hit enter to search. For more options:")
                    .color(Color::Muted)
                    .mb_2(),
            )
            .child(
                Button::new("filter-paths", "Include/exclude specific paths")
                    .start_icon(Icon::new(IconName::Filter).size(IconSize::Small))
                    .key_binding(KeyBinding::for_action_in(&ToggleFilters, &focus_handle, cx))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(ToggleFilters.boxed_clone(), cx)
                    }),
            )
            .child(
                Button::new("find-replace", "Find and replace")
                    .start_icon(Icon::new(IconName::Replace).size(IconSize::Small))
                    .key_binding(KeyBinding::for_action_in(&ToggleReplace, &focus_handle, cx))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(ToggleReplace.boxed_clone(), cx)
                    }),
            )
            .child(
                Button::new("regex", "Match with regex")
                    .start_icon(Icon::new(IconName::Regex).size(IconSize::Small))
                    .key_binding(KeyBinding::for_action_in(&ToggleRegex, &focus_handle, cx))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(ToggleRegex.boxed_clone(), cx)
                    }),
            )
            .child(
                Button::new("match-case", "Match case")
                    .start_icon(Icon::new(IconName::CaseSensitive).size(IconSize::Small))
                    .key_binding(KeyBinding::for_action_in(
                        &ToggleCaseSensitive,
                        &focus_handle,
                        cx,
                    ))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(ToggleCaseSensitive.boxed_clone(), cx)
                    }),
            )
            .child(
                Button::new("match-whole-words", "Match whole words")
                    .start_icon(Icon::new(IconName::WholeWord).size(IconSize::Small))
                    .key_binding(KeyBinding::for_action_in(
                        &ToggleWholeWord,
                        &focus_handle,
                        cx,
                    ))
                    .on_click(|_event, window, cx| {
                        window.dispatch_action(ToggleWholeWord.boxed_clone(), cx)
                    }),
            )
    }

    fn border_color_for(&self, panel: InputPanel, cx: &App) -> Hsla {
        if self.panels_with_errors.contains_key(&panel) {
            Color::Error.color(cx)
        } else {
            cx.theme().colors().border
        }
    }

    fn move_focus_to_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.results_editor.focus_handle(cx).is_focused(window)
            && !self.entity.read(cx).match_ranges.is_empty()
        {
            cx.stop_propagation();
            self.focus_results_editor(window, cx)
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn results_editor(&self) -> &Entity<Editor> {
        &self.results_editor
    }

    fn adjust_query_regex_language(&self, cx: &mut App) {
        let enable = self.search_options.contains(SearchOptions::REGEX);
        let query_buffer = self
            .query_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .expect("query editor should be backed by a singleton buffer");
        if enable {
            if let Some(regex_language) = self.regex_language.clone() {
                query_buffer.update(cx, |query_buffer, cx| {
                    query_buffer.set_language(Some(regex_language), cx);
                })
            }
        } else {
            query_buffer.update(cx, |query_buffer, cx| {
                query_buffer.set_language(None, cx);
            })
        }
    }
}

pub(crate) fn buffer_search_query(
    workspace: &mut Workspace,
    item: &dyn ItemHandle,
    cx: &mut Context<Workspace>,
) -> Option<String> {
    let buffer_search_bar = workspace
        .pane_for(item)
        .and_then(|pane| {
            pane.read(cx)
                .toolbar()
                .read(cx)
                .item_of_type::<BufferSearchBar>()
        })?
        .read(cx);
    if buffer_search_bar.query_editor_focused() {
        let buffer_search_query = buffer_search_bar.query(cx);
        if !buffer_search_query.is_empty() {
            return Some(buffer_search_query);
        }
    }
    None
}

impl Default for ProjectSearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectSearchBar {
    pub fn new() -> Self {
        Self {
            active_project_search: None,
            subscription: None,
        }
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                if !search_view
                    .replacement_editor
                    .focus_handle(cx)
                    .is_focused(window)
                {
                    cx.stop_propagation();
                    search_view
                        .prompt_to_save_if_dirty_then_search(window, cx)
                        .detach_and_log_err(cx);
                }
            });
        }
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_field(Direction::Next, window, cx);
    }

    fn backtab(&mut self, _: &Backtab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_field(Direction::Prev, window, cx);
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                search_view.query_editor.focus_handle(cx).focus(window, cx);
            });
        }
    }

    fn cycle_field(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let active_project_search = match &self.active_project_search {
            Some(active_project_search) => active_project_search,
            None => return,
        };

        active_project_search.update(cx, |project_view, cx| {
            let mut views = vec![project_view.query_editor.focus_handle(cx)];
            if project_view.replace_enabled {
                views.push(project_view.replacement_editor.focus_handle(cx));
            }
            if project_view.filters_enabled {
                views.extend([
                    project_view.included_files_editor.focus_handle(cx),
                    project_view.excluded_files_editor.focus_handle(cx),
                ]);
            }
            let current_index = match views.iter().position(|focus| focus.is_focused(window)) {
                Some(index) => index,
                None => return,
            };

            let new_index = match direction {
                Direction::Next => (current_index + 1) % views.len(),
                Direction::Prev if current_index == 0 => views.len() - 1,
                Direction::Prev => (current_index - 1) % views.len(),
            };
            let next_focus_handle = &views[new_index];
            window.focus(next_focus_handle, cx);
            cx.stop_propagation();
        });
    }

    pub(crate) fn toggle_search_option(
        &mut self,
        option: SearchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_project_search.is_none() {
            return false;
        }

        cx.spawn_in(window, async move |this, cx| {
            let task = this.update_in(cx, |this, window, cx| {
                let search_view = this.active_project_search.as_ref()?;
                search_view.update(cx, |search_view, cx| {
                    search_view.toggle_search_option(option, cx);
                    search_view
                        .entity
                        .read(cx)
                        .active_query
                        .is_some()
                        .then(|| search_view.prompt_to_save_if_dirty_then_search(window, cx))
                })
            })?;
            if let Some(task) = task {
                task.await?;
            }
            this.update(cx, |_, cx| {
                cx.notify();
            })?;
            anyhow::Ok(())
        })
        .detach();
        true
    }

    fn toggle_replace(&mut self, _: &ToggleReplace, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.active_project_search {
            search.update(cx, |this, cx| {
                this.replace_enabled = !this.replace_enabled;
                let editor_to_focus = if this.replace_enabled {
                    this.replacement_editor.focus_handle(cx)
                } else {
                    this.query_editor.focus_handle(cx)
                };
                window.focus(&editor_to_focus, cx);
                cx.notify();
            });
        }
    }

    fn toggle_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                search_view.toggle_filters(cx);
                search_view
                    .included_files_editor
                    .update(cx, |_, cx| cx.notify());
                search_view
                    .excluded_files_editor
                    .update(cx, |_, cx| cx.notify());
                window.refresh();
                cx.notify();
            });
            cx.notify();
            true
        } else {
            false
        }
    }

    fn toggle_opened_only(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.active_project_search.is_none() {
            return false;
        }

        cx.spawn_in(window, async move |this, cx| {
            let task = this.update_in(cx, |this, window, cx| {
                let search_view = this.active_project_search.as_ref()?;
                search_view.update(cx, |search_view, cx| {
                    search_view.toggle_opened_only(window, cx);
                    search_view
                        .entity
                        .read(cx)
                        .active_query
                        .is_some()
                        .then(|| search_view.prompt_to_save_if_dirty_then_search(window, cx))
                })
            })?;
            if let Some(task) = task {
                task.await?;
            }
            this.update(cx, |_, cx| {
                cx.notify();
            })?;
            anyhow::Ok(())
        })
        .detach();
        true
    }

    fn is_opened_only_enabled(&self, cx: &App) -> bool {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.read(cx).included_opened_only
        } else {
            false
        }
    }

    fn move_focus_to_results(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                search_view.move_focus_to_results(window, cx);
            });
            cx.notify();
        }
    }

    fn next_history_query(
        &mut self,
        _: &NextHistoryQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                for (editor, kind) in [
                    (search_view.query_editor.clone(), SearchInputKind::Query),
                    (
                        search_view.included_files_editor.clone(),
                        SearchInputKind::Include,
                    ),
                    (
                        search_view.excluded_files_editor.clone(),
                        SearchInputKind::Exclude,
                    ),
                ] {
                    if editor.focus_handle(cx).is_focused(window) {
                        if !should_navigate_history(&editor, HistoryNavigationDirection::Next, cx) {
                            cx.propagate();
                            return;
                        }

                        let new_query = search_view.entity.update(cx, |model, cx| {
                            let project = model.project.clone();

                            if let Some(new_query) = project.update(cx, |project, _| {
                                project
                                    .search_history_mut(kind)
                                    .next(model.cursor_mut(kind))
                                    .map(str::to_string)
                            }) {
                                Some(new_query)
                            } else {
                                model.cursor_mut(kind).take_draft()
                            }
                        });
                        if let Some(new_query) = new_query {
                            search_view.set_search_editor(kind, &new_query, window, cx);
                        }
                    }
                }
            });
        }
    }

    fn previous_history_query(
        &mut self,
        _: &PreviousHistoryQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search_view) = self.active_project_search.as_ref() {
            search_view.update(cx, |search_view, cx| {
                for (editor, kind) in [
                    (search_view.query_editor.clone(), SearchInputKind::Query),
                    (
                        search_view.included_files_editor.clone(),
                        SearchInputKind::Include,
                    ),
                    (
                        search_view.excluded_files_editor.clone(),
                        SearchInputKind::Exclude,
                    ),
                ] {
                    if editor.focus_handle(cx).is_focused(window) {
                        if !should_navigate_history(
                            &editor,
                            HistoryNavigationDirection::Previous,
                            cx,
                        ) {
                            cx.propagate();
                            return;
                        }

                        if editor.read(cx).text(cx).is_empty()
                            && let Some(new_query) = search_view
                                .entity
                                .read(cx)
                                .project
                                .read(cx)
                                .search_history(kind)
                                .current(search_view.entity.read(cx).cursor(kind))
                                .map(str::to_string)
                        {
                            search_view.set_search_editor(kind, &new_query, window, cx);
                            return;
                        }

                        let current_query = editor.read(cx).text(cx);
                        if let Some(new_query) = search_view.entity.update(cx, |model, cx| {
                            let project = model.project.clone();
                            project.update(cx, |project, _| {
                                project
                                    .search_history_mut(kind)
                                    .previous(model.cursor_mut(kind), &current_query)
                                    .map(str::to_string)
                            })
                        }) {
                            search_view.set_search_editor(kind, &new_query, window, cx);
                        }
                    }
                }
            });
        }
    }

    fn select_next_match(
        &mut self,
        _: &SelectNextMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.active_project_search.as_ref() {
            search.update(cx, |this, cx| {
                this.select_match(Direction::Next, window, cx);
            })
        }
    }

    fn select_prev_match(
        &mut self,
        _: &SelectPreviousMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.active_project_search.as_ref() {
            search.update(cx, |this, cx| {
                this.select_match(Direction::Prev, window, cx);
            })
        }
    }

    fn open_text_finder(
        &mut self,
        _: &OpenTextFinder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(search) = &self.active_project_search else {
            tracing::warn!("active_project_search was none");
            return;
        };

        TextFinder::open_from_project_search(Entity::clone(search), window, cx).detach();
    }
}

impl Render for ProjectSearchBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(search) = self.active_project_search.clone() else {
            return div().into_any_element();
        };
        let search = search.read(cx);
        let focus_handle = search.focus_handle(cx);

        let container_width = window.viewport_size().width;
        let input_width = SearchInputWidth::calc_width(container_width);

        let input_base_styles = |panel: InputPanel| {
            input_base_styles(search.border_color_for(panel, cx), |div| match panel {
                InputPanel::Query | InputPanel::Replacement => div.w(input_width),
                InputPanel::Include | InputPanel::Exclude => div.flex_grow_1(),
            })
        };
        let theme_colors = cx.theme().colors();
        let project_search = search.entity.read(cx);
        let limit_reached = project_search.search_state.limit_reached();
        let is_search_underway = project_search.pending_search.is_some();

        let color_override = match (
            project_search.search_state,
            &project_search.active_query,
            &project_search.last_search_query_text,
        ) {
            (
                SearchState::Completed(SearchCompletion::NoResults),
                Some(query),
                Some(previous_query),
            ) if query.as_str() == previous_query => Some(Color::Error),
            _ => None,
        };

        let match_text = search
            .active_match_index
            .and_then(|index| {
                let index = index + 1;
                let match_quantity = project_search.match_ranges.len();
                if match_quantity > 0 {
                    debug_assert!(match_quantity >= index);
                    if limit_reached {
                        Some(format!("{index}/{match_quantity}+"))
                    } else {
                        Some(format!("{index}/{match_quantity}"))
                    }
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "0/0".to_string());

        let query_focus = search.query_editor.focus_handle(cx);

        let query_column = input_base_styles(InputPanel::Query)
            .on_action(cx.listener(|this, action, window, cx| this.confirm(action, window, cx)))
            .on_action(cx.listener(|this, action, window, cx| {
                this.previous_history_query(action, window, cx)
            }))
            .on_action(
                cx.listener(|this, action, window, cx| this.next_history_query(action, window, cx)),
            )
            .child(div().flex_1().py_1().child(render_text_input(
                &search.query_editor,
                color_override,
                cx,
            )))
            .child(
                h_flex()
                    .gap_1()
                    .child(SearchOption::CaseSensitive.as_button(
                        search.search_options,
                        SearchSource::Project(cx),
                        focus_handle.clone(),
                    ))
                    .child(SearchOption::WholeWord.as_button(
                        search.search_options,
                        SearchSource::Project(cx),
                        focus_handle.clone(),
                    ))
                    .child(SearchOption::Regex.as_button(
                        search.search_options,
                        SearchSource::Project(cx),
                        focus_handle.clone(),
                    )),
            );

        let matches_column = h_flex()
            .ml_1()
            .pl_1p5()
            .border_l_1()
            .border_color(theme_colors.border_variant)
            .child(render_action_button(
                "project-search-nav-button",
                IconName::ChevronLeft,
                search
                    .active_match_index
                    .is_none()
                    .then_some(ActionButtonState::Disabled),
                "Select Previous Match",
                &SelectPreviousMatch,
                query_focus.clone(),
            ))
            .child(render_action_button(
                "project-search-nav-button",
                IconName::ChevronRight,
                search
                    .active_match_index
                    .is_none()
                    .then_some(ActionButtonState::Disabled),
                "Select Next Match",
                &SelectNextMatch,
                query_focus.clone(),
            ))
            .child(
                div()
                    .id("matches")
                    .ml_2()
                    .min_w(rems_from_px(40.))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Label::new(match_text)
                                    .size(LabelSize::Small)
                                    .when(search.active_match_index.is_some(), |this| {
                                        this.color(Color::Disabled)
                                    }),
                            )
                            .when(is_search_underway, |this| {
                                this.child(
                                    Icon::new(IconName::ArrowCircle)
                                        .color(Color::Accent)
                                        .size(IconSize::Small)
                                        .with_rotate_animation(2)
                                        .into_any_element(),
                                )
                            }),
                    )
                    .when(limit_reached, |this| {
                        this.tooltip(Tooltip::text(
                            "Search Limits Reached\nTry narrowing your search",
                        ))
                    }),
            );

        let mode_column = h_flex()
            .gap_1()
            .min_w_64()
            .child(
                IconButton::new("project-search-filter-button", IconName::Filter)
                    .shape(IconButtonShape::Square)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action("Toggle Filters", &ToggleFilters, cx)
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_filters(window, cx);
                    }))
                    .toggle_state(
                        self.active_project_search
                            .as_ref()
                            .map(|search| search.read(cx).filters_enabled)
                            .unwrap_or_default(),
                    )
                    .tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Toggle Filters",
                                &ToggleFilters,
                                &focus_handle,
                                cx,
                            )
                        }
                    }),
            )
            .child(render_action_button(
                "project-search",
                IconName::Replace,
                self.active_project_search
                    .as_ref()
                    .map(|search| search.read(cx).replace_enabled)
                    .and_then(|enabled| enabled.then_some(ActionButtonState::Toggled)),
                "Toggle Replace",
                &ToggleReplace,
                focus_handle.clone(),
            ))
            .child(matches_column);

        let is_collapsed = search.results_editor.read(cx).has_any_buffer_folded(cx);

        let (icon, tooltip_label) = if is_collapsed {
            (IconName::ChevronUpDown, "Expand All Search Results")
        } else {
            (IconName::ChevronDownUp, "Collapse All Search Results")
        };

        let expand_button = IconButton::new("project-search-collapse-expand", icon)
            .shape(IconButtonShape::Square)
            .tooltip(move |_, cx| {
                Tooltip::for_action_in(
                    tooltip_label,
                    &ToggleAllSearchResults,
                    &query_focus.clone(),
                    cx,
                )
            })
            .on_click(cx.listener(|this, _, window, cx| {
                if let Some(active_view) = &this.active_project_search {
                    active_view.update(cx, |active_view, cx| {
                        active_view.toggle_all_search_results(&ToggleAllSearchResults, window, cx);
                    })
                }
            }));

        let search_line = h_flex()
            .pl_0p5()
            .w_full()
            .gap_2()
            .child(expand_button)
            .child(query_column)
            .child(mode_column);

        let replace_line = search.replace_enabled.then(|| {
            let replace_column = input_base_styles(InputPanel::Replacement).child(
                div().flex_1().py_1().child(render_text_input(
                    &search.replacement_editor,
                    None,
                    cx,
                )),
            );

            let focus_handle = search.replacement_editor.read(cx).focus_handle(cx);
            let replace_actions = h_flex()
                .min_w_64()
                .gap_1()
                .child(render_action_button(
                    "project-search-replace-button",
                    IconName::ReplaceNext,
                    is_search_underway.then_some(ActionButtonState::Disabled),
                    "Replace Next Match",
                    &ReplaceNext,
                    focus_handle.clone(),
                ))
                .child(render_action_button(
                    "project-search-replace-button",
                    IconName::ReplaceAll,
                    Default::default(),
                    "Replace All Matches",
                    &ReplaceAll,
                    focus_handle,
                ));

            h_flex()
                .w_full()
                .gap_2()
                .child(alignment_element())
                .child(replace_column)
                .child(replace_actions)
        });

        let filter_line = search.filters_enabled.then(|| {
            let include = input_base_styles(InputPanel::Include)
                .on_action(cx.listener(|this, action, window, cx| {
                    this.previous_history_query(action, window, cx)
                }))
                .on_action(cx.listener(|this, action, window, cx| {
                    this.next_history_query(action, window, cx)
                }))
                .child(render_text_input(&search.included_files_editor, None, cx));
            let exclude = input_base_styles(InputPanel::Exclude)
                .on_action(cx.listener(|this, action, window, cx| {
                    this.previous_history_query(action, window, cx)
                }))
                .on_action(cx.listener(|this, action, window, cx| {
                    this.next_history_query(action, window, cx)
                }))
                .child(render_text_input(&search.excluded_files_editor, None, cx));
            let mode_column = h_flex()
                .gap_1()
                .min_w_64()
                .child(
                    IconButton::new("project-search-opened-only", IconName::FolderSearch)
                        .shape(IconButtonShape::Square)
                        .toggle_state(self.is_opened_only_enabled(cx))
                        .tooltip(Tooltip::text("Only Search Open Files"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_opened_only(window, cx);
                        })),
                )
                .child(SearchOption::IncludeIgnored.as_button(
                    search.search_options,
                    SearchSource::Project(cx),
                    focus_handle,
                ));

            h_flex()
                .w_full()
                .gap_2()
                .child(alignment_element())
                .child(
                    h_flex()
                        .w(input_width)
                        .gap_2()
                        .child(include)
                        .child(exclude),
                )
                .child(mode_column)
        });

        let mut key_context = KeyContext::default();
        key_context.add("ProjectSearchBar");
        if search
            .replacement_editor
            .focus_handle(cx)
            .is_focused(window)
        {
            key_context.add("in_replace");
        }

        let query_error_line = search
            .panels_with_errors
            .get(&InputPanel::Query)
            .map(|error| {
                Label::new(error)
                    .size(LabelSize::Small)
                    .color(Color::Error)
                    .mt_neg_1()
                    .ml_2()
            });

        let filter_error_line = search
            .panels_with_errors
            .get(&InputPanel::Include)
            .or_else(|| search.panels_with_errors.get(&InputPanel::Exclude))
            .map(|error| {
                Label::new(error)
                    .size(LabelSize::Small)
                    .color(Color::Error)
                    .mt_neg_1()
                    .ml_2()
            });

        v_flex()
            .gap_2()
            .w_full()
            .key_context(key_context)
            .on_action(cx.listener(|this, _: &ToggleFocus, window, cx| {
                this.move_focus_to_results(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleFilters, window, cx| {
                this.toggle_filters(window, cx);
            }))
            .capture_action(cx.listener(Self::tab))
            .capture_action(cx.listener(Self::backtab))
            .on_action(cx.listener(|this, action, window, cx| this.confirm(action, window, cx)))
            .on_action(cx.listener(|this, action, window, cx| {
                this.toggle_replace(action, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleWholeWord, window, cx| {
                this.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCaseSensitive, window, cx| {
                this.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx);
            }))
            .on_action(cx.listener(|this, action, window, cx| {
                if let Some(search) = this.active_project_search.as_ref() {
                    search.update(cx, |this, cx| {
                        this.replace_next(action, window, cx);
                    })
                }
            }))
            .on_action(cx.listener(|this, action, window, cx| {
                if let Some(search) = this.active_project_search.as_ref() {
                    search.update(cx, |this, cx| {
                        this.replace_all(action, window, cx);
                    })
                }
            }))
            .when(search.filters_enabled, |this| {
                this.on_action(cx.listener(|this, _: &ToggleIncludeIgnored, window, cx| {
                    this.toggle_search_option(SearchOptions::INCLUDE_IGNORED, window, cx);
                }))
            })
            .on_action(cx.listener(Self::select_next_match))
            .on_action(cx.listener(Self::select_prev_match))
            .on_action(cx.listener(Self::open_text_finder))
            .child(search_line)
            .children(query_error_line)
            .children(replace_line)
            .children(filter_line)
            .children(filter_error_line)
            .into_any_element()
    }
}

impl EventEmitter<ToolbarItemEvent> for ProjectSearchBar {}

impl ToolbarItemView for ProjectSearchBar {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        cx.notify();
        self.subscription = None;
        self.active_project_search = None;
        if let Some(search) = active_pane_item.and_then(|i| i.downcast::<ProjectSearchView>()) {
            self.subscription = Some(cx.observe(&search, |_, _, cx| cx.notify()));
            self.active_project_search = Some(search);
            ToolbarItemLocation::PrimaryLeft {}
        } else {
            ToolbarItemLocation::Hidden
        }
    }
}

fn register_workspace_action<A: Action>(
    workspace: &mut Workspace,
    callback: fn(&mut ProjectSearchBar, &A, &mut Window, &mut Context<ProjectSearchBar>),
) {
    workspace.register_action(move |workspace, action: &A, window, cx| {
        if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
            cx.propagate();
            return;
        }

        workspace.active_pane().update(cx, |pane, cx| {
            pane.toolbar().update(cx, move |workspace, cx| {
                if let Some(search_bar) = workspace.item_of_type::<ProjectSearchBar>() {
                    search_bar.update(cx, move |search_bar, cx| {
                        if search_bar.active_project_search.is_some() {
                            callback(search_bar, action, window, cx);
                            cx.notify();
                        } else {
                            cx.propagate();
                        }
                    });
                }
            });
        })
    });
}

fn register_workspace_action_for_present_search<A: Action>(
    workspace: &mut Workspace,
    callback: fn(&mut Workspace, &A, &mut Window, &mut Context<Workspace>),
) {
    workspace.register_action(move |workspace, action: &A, window, cx| {
        if workspace.has_active_modal(window, cx) && !workspace.hide_modal(window, cx) {
            cx.propagate();
            return;
        }

        let should_notify = workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<ProjectSearchBar>()
            .map(|search_bar| search_bar.read(cx).active_project_search.is_some())
            .unwrap_or(false);
        if should_notify {
            callback(workspace, action, window, cx);
            cx.notify();
        } else {
            cx.propagate();
        }
    });
}

#[cfg(any(test, feature = "test-support"))]
pub fn perform_project_search(
    search_view: &Entity<ProjectSearchView>,
    text: impl Into<std::sync::Arc<str>>,
    cx: &mut gpui::VisualTestContext,
) {
    cx.run_until_parked();
    search_view.update_in(cx, |search_view, window, cx| {
        search_view.query_editor.update(cx, |query_editor, cx| {
            query_editor.set_text(text, window, cx)
        });
        search_view.search(cx);
    });
    cx.run_until_parked();
}

#[cfg(test)]
mod tests;
