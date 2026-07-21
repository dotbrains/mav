use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use acp_thread::{
    AcpThread, AcpThreadEvent, AgentThreadEntry, AssistantMessageChunk, ContentBlock,
    ToolCallContent,
};
use collections::HashMap;
use editor::{
    Editor, EditorElement, EditorEvent, EditorStyle, HighlightKey, SelectionEffects,
    scroll::Autoscroll,
};
use gpui::{
    Action, Entity, EntityId, EventEmitter, FocusHandle, Focusable, KeyContext, Subscription, Task,
    TextStyle, WeakEntity, actions, prelude::*,
};
use markdown::Markdown;
use multi_buffer::{Anchor, MultiBufferOffset, MultiBufferSnapshot};
use project::search::SearchQuery;
use search::{SearchOption, SearchOptions, SearchSource};
use settings::Settings as _;
use theme_settings::ThemeSettings;
use ui::{IconButtonShape, Tooltip, prelude::*};
use util::paths::PathMatcher;

use crate::entry_view_state::EntryViewState;

mod rendering;
mod search_logic;
mod search_model;
use search_model::*;

actions!(
    agent,
    [
        /// Closes the thread search bar.
        DismissThreadSearch,
        /// Selects the next thread search match.
        SelectNextThreadMatch,
        /// Selects the previous thread search match.
        SelectPreviousThreadMatch,
    ]
);

/// Debounce for streaming thread updates, which can fire once per streamed
/// chunk. Query edits are handled immediately instead (see the query editor
/// subscription in `ThreadSearchBar::new`).
pub(super) const SEARCH_UPDATE_DEBOUNCE: Duration = Duration::from_millis(150);

pub struct ThreadSearchBar {
    pub(super) query_editor: Entity<Editor>,
    options: SearchOptions,
    matches: Vec<ThreadMatch>,
    active_match: Option<usize>,
    query_error: bool,
    query_error_message: Option<SharedString>,
    highlighted_markdowns: Vec<WeakEntity<Markdown>>,
    highlighted_editors: Vec<WeakEntity<Editor>>,
    thread: Entity<AcpThread>,
    entry_view_state: Entity<EntryViewState>,
    on_activate_match: Arc<dyn Fn(usize, &mut Window, &mut App)>,
    is_active: bool,
    _update_matches_task: Option<Task<()>>,
    _search_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

pub enum ThreadSearchBarEvent {
    Dismissed,
}

impl EventEmitter<ThreadSearchBarEvent> for ThreadSearchBar {}

impl Focusable for ThreadSearchBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.query_editor.focus_handle(cx)
    }
}

impl ThreadSearchBar {
    pub fn new(
        thread: Entity<AcpThread>,
        entry_view_state: Entity<EntryViewState>,
        on_activate_match: Arc<dyn Fn(usize, &mut Window, &mut App)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Search this thread…", window, cx);
            editor
        });
        let editor_subscription = cx.subscribe_in(
            &query_editor,
            window,
            |this, _editor, event: &EditorEvent, window, cx| {
                if matches!(
                    event,
                    EditorEvent::Edited { .. } | EditorEvent::BufferEdited
                ) {
                    // Re-scan immediately so typing feels responsive
                    this._update_matches_task = None;
                    this.update_matches(window, cx);
                }
            },
        );
        let thread_subscription = cx.subscribe_in(
            &thread,
            window,
            |this, _thread, event: &AcpThreadEvent, window, cx| {
                if this.is_active
                    && matches!(
                        event,
                        AcpThreadEvent::NewEntry
                            | AcpThreadEvent::EntryUpdated(_)
                            | AcpThreadEvent::EntriesRemoved(_)
                    )
                {
                    this.schedule_update_matches(window, cx);
                }
            },
        );
        cx.on_release(|this, cx| {
            this.clear_highlights_impl(cx);
        })
        .detach();
        Self {
            query_editor,
            options: SearchOptions::NONE,
            matches: Vec::new(),
            active_match: None,
            query_error: false,
            query_error_message: None,
            highlighted_markdowns: Vec::new(),
            highlighted_editors: Vec::new(),
            thread,
            entry_view_state,
            on_activate_match,
            is_active: false,
            _update_matches_task: None,
            _search_task: None,
            _subscriptions: vec![editor_subscription, thread_subscription],
        }
    }

    pub fn focus_and_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_active = true;
        self.focus_query_and_select_all(window, cx);
        self.update_matches(window, cx);
    }

    fn focus_query_and_select_all(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self.query_editor.focus_handle(cx);
        focus_handle.focus(window, cx);
        self.query_editor.update(cx, |editor, cx| {
            editor.select_all(&editor::actions::SelectAll, window, cx);
        });
    }

    fn schedule_update_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._update_matches_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SEARCH_UPDATE_DEBOUNCE).await;
            this.update_in(cx, |this, window, cx| this.update_matches(window, cx))
                .ok();
        }));
    }

    #[cfg(test)]
    pub(super) fn match_count(&self) -> usize {
        self.matches.len()
    }

    #[cfg(test)]
    pub(super) fn active_match_index(&self) -> Option<usize> {
        self.active_match
    }

    pub fn active_match_text(&self, cx: &App) -> Option<String> {
        if self.query_editor.read(cx).text(cx).is_empty() {
            return None;
        }
        match self.active_match {
            Some(ix) => Some(format!("{}/{}", ix + 1, self.matches.len())),
            None => Some(format!("0/{}", self.matches.len())),
        }
    }

    fn current_query(&self, cx: &App) -> String {
        self.query_editor.read(cx).text(cx)
    }

    fn build_query(&self, cx: &App) -> (Option<Arc<SearchQuery>>, Option<SharedString>) {
        let text = self.current_query(cx);
        if text.is_empty() {
            return (None, None);
        }
        let whole_word = self.options.contains(SearchOptions::WHOLE_WORD);
        let case_sensitive = self.options.contains(SearchOptions::CASE_SENSITIVE);
        let result = if self.options.contains(SearchOptions::REGEX) {
            SearchQuery::regex(
                text,
                whole_word,
                case_sensitive,
                false,
                false,
                PathMatcher::default(),
                PathMatcher::default(),
                false,
                None,
            )
        } else {
            SearchQuery::text(
                text,
                whole_word,
                case_sensitive,
                false,
                PathMatcher::default(),
                PathMatcher::default(),
                false,
                None,
            )
        };
        match result {
            Ok(q) => (Some(Arc::new(q)), None),
            Err(err) => (None, Some(SharedString::from(err.to_string()))),
        }
    }

    pub(super) fn select_next_match(
        &mut self,
        _: &SelectNextThreadMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.matches.is_empty() {
            return;
        }
        let next = match self.active_match {
            Some(ix) => (ix + 1) % self.matches.len(),
            None => 0,
        };
        self.activate_match(next, true, window, cx);
    }

    pub(super) fn select_prev_match(
        &mut self,
        _: &SelectPreviousThreadMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.matches.is_empty() {
            return;
        }
        let prev = match self.active_match {
            Some(ix) => {
                if ix == 0 {
                    self.matches.len() - 1
                } else {
                    ix - 1
                }
            }
            None => self.matches.len() - 1,
        };
        self.activate_match(prev, true, window, cx);
    }

    fn dismiss(&mut self, _: &DismissThreadSearch, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_highlights(cx);
        cx.emit(ThreadSearchBarEvent::Dismissed);
    }

    pub fn clear_highlights(&mut self, cx: &mut Context<Self>) {
        self.clear_highlights_impl(cx);
        cx.notify();
    }

    fn clear_highlights_impl(&mut self, cx: &mut App) {
        self.clear_results(cx);
        self.is_active = false;
        self._update_matches_task = None;
    }

    /// Drops all painted highlights, recorded matches, and any in-flight scan.
    fn clear_results(&mut self, cx: &mut App) {
        self.clear_match_highlights(cx);
        self.matches.clear();
        self.active_match = None;
        self._search_task = None;
    }

    fn clear_match_highlights(&mut self, cx: &mut App) {
        for weak in self.highlighted_markdowns.drain(..) {
            if let Some(md) = weak.upgrade() {
                md.update(cx, |md, cx| md.clear_search_highlights(cx));
            }
        }
        for weak in self.highlighted_editors.drain(..) {
            if let Some(editor) = weak.upgrade() {
                editor.update(cx, |editor, cx| {
                    editor.clear_background_highlights(HighlightKey::BufferSearchHighlights, cx);
                });
            }
        }
    }

    pub(super) fn toggle_case_sensitive(
        &mut self,
        _: &search::ToggleCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options.toggle(SearchOptions::CASE_SENSITIVE);
        self.update_matches(window, cx);
    }

    pub(super) fn toggle_whole_word(
        &mut self,
        _: &search::ToggleWholeWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options.toggle(SearchOptions::WHOLE_WORD);
        self.update_matches(window, cx);
    }

    pub(super) fn toggle_regex(
        &mut self,
        _: &search::ToggleRegex,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.options.toggle(SearchOptions::REGEX);
        self.update_matches(window, cx);
    }

    pub(super) fn focus_search(
        &mut self,
        _: &search::FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_query_and_select_all(window, cx);
    }
}
