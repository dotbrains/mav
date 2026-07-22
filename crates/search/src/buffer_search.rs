mod registrar;

use crate::{
    FocusSearch, NextHistoryQuery, PreviousHistoryQuery, ReplaceAll, ReplaceNext, SearchOption,
    SearchOptions, SearchSource, SelectAllMatches, SelectNextMatch, SelectPreviousMatch,
    ToggleCaseSensitive, ToggleRegex, ToggleReplace, ToggleSelection, ToggleWholeWord,
    buffer_search::registrar::WithResultsOrExternalQuery,
    search_bar::{
        ActionButtonState, HistoryNavigationDirection, alignment_element,
        filter_search_results_input, input_base_styles, render_action_button, render_text_input,
        should_navigate_history,
    },
};
use any_vec::AnyVec;
use collections::HashMap;
use editor::{
    Editor, EditorSettings, MultiBufferOffset, SplittableEditor, ToggleSplitDiff,
    actions::{Backtab, FoldAll, Tab, ToggleFoldAll, UnfoldAll},
    scroll::Autoscroll,
};
use futures::channel::oneshot;
use gpui::{
    Action as _, App, ClickEvent, Context, Entity, EventEmitter, Focusable,
    InteractiveElement as _, IntoElement, KeyContext, ParentElement as _, Render, ScrollHandle,
    Styled, Subscription, Task, TaskExt, WeakEntity, Window, div,
};
use language::{Language, LanguageRegistry};
use project::{
    search::SearchQuery,
    search_history::{SearchHistory, SearchHistoryCursor},
};

use fs::Fs;
use mav_actions::{
    OpenSettingsAt, outline::ToggleOutline, workspace::CopyPath, workspace::CopyRelativePath,
};
use settings::{DiffViewStyle, SeedQuerySetting, Settings, update_settings_file};
use std::{any::TypeId, sync::Arc};

use ui::{
    BASE_REM_SIZE_IN_PX, IconButtonShape, PlatformStyle, TextSize, Tooltip, prelude::*,
    render_modifiers, utils::SearchInputWidth,
};
use util::{ResultExt, paths::PathMatcher};
use workspace::{
    ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, Workspace,
    item::{ItemBufferKind, ItemHandle},
    searchable::{
        Direction, FilteredSearchRange, SearchEvent, SearchToken, SearchableItemHandle,
        WeakSearchableItemHandle,
    },
};

pub use registrar::{DivRegistrar, register_pane_search_actions};
use registrar::{ForDeployed, ForDismissed, SearchActionsRegistrar};

const MAX_BUFFER_SEARCH_HISTORY_SIZE: usize = 50;

pub use mav_actions::buffer_search::{
    Deploy, DeployReplace, Dismiss, FocusEditor, UseSelectionForFind,
};

pub enum Event {
    UpdateLocation,
    Dismissed,
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| BufferSearchBar::register(workspace))
        .detach();
}

pub struct BufferSearchBar {
    query_editor: Entity<Editor>,
    query_editor_focused: bool,
    replacement_editor: Entity<Editor>,
    replacement_editor_focused: bool,
    active_searchable_item: Option<Box<dyn SearchableItemHandle>>,
    active_match_index: Option<usize>,
    #[cfg(target_os = "macos")]
    active_searchable_item_subscriptions: Option<[Subscription; 2]>,
    #[cfg(not(target_os = "macos"))]
    active_searchable_item_subscriptions: Option<Subscription>,
    #[cfg(target_os = "macos")]
    pending_external_query: Option<(String, SearchOptions)>,
    active_search: Option<Arc<SearchQuery>>,
    searchable_items_with_matches:
        HashMap<Box<dyn WeakSearchableItemHandle>, (AnyVec<dyn Send>, SearchToken)>,
    pending_search: Option<Task<()>>,
    search_options: SearchOptions,
    default_options: SearchOptions,
    configured_options: SearchOptions,
    query_error: Option<String>,
    dismissed: bool,
    search_history: SearchHistory,
    search_history_cursor: SearchHistoryCursor,
    replace_enabled: bool,
    selection_search_enabled: Option<FilteredSearchRange>,
    scroll_handle: ScrollHandle,
    regex_language: Option<Arc<Language>>,
    splittable_editor: Option<WeakEntity<SplittableEditor>>,
    _splittable_editor_subscription: Option<Subscription>,
}

impl EventEmitter<Event> for BufferSearchBar {}
impl EventEmitter<workspace::ToolbarItemEvent> for BufferSearchBar {}

mod deploy;
mod events;
mod history_replace;
mod lifecycle;
mod matches;
mod options;
mod render;
mod render_helpers;
mod toolbar;

#[cfg(test)]
mod tests;
