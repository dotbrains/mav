use super::*;

pub struct ProjectSearch {
    pub(crate) project: Entity<Project>,
    pub excerpts: Entity<MultiBuffer>,
    pub pending_search: Option<Task<Option<SearchResults<SearchResult>>>>,
    pub match_ranges: Vec<Range<Anchor>>,
    pub(crate) active_query: Option<SearchQuery>,
    pub(super) last_search_query_text: Option<String>,
    pub search_id: usize,
    pub(super) search_state: SearchState,
    pub(super) search_history_cursor: SearchHistoryCursor,
    pub(super) search_included_history_cursor: SearchHistoryCursor,
    pub(super) search_excluded_history_cursor: SearchHistoryCursor,
    pub project_search_turning_into_text_finder: Arc<AtomicBool>,
    pub(super) _excerpts_subscription: Subscription,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SearchState {
    #[default]
    Idle,
    Running(SearchActivity),
    Completed(SearchCompletion),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchActivity {
    Searching,
    WaitingForScan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchCompletion {
    NoResults,
    Results { limit_reached: bool },
}

impl SearchState {
    pub(super) fn limit_reached(self) -> bool {
        matches!(
            self,
            SearchState::Completed(SearchCompletion::Results {
                limit_reached: true
            })
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum InputPanel {
    Query,
    Replacement,
    Exclude,
    Include,
}

pub struct ProjectSearchView {
    pub(crate) workspace: WeakEntity<Workspace>,
    pub(super) focus_handle: FocusHandle,
    pub(crate) entity: Entity<ProjectSearch>,
    pub(super) query_editor: Entity<Editor>,
    pub(super) replacement_editor: Entity<Editor>,
    pub(super) results_editor: Entity<Editor>,
    pub(crate) search_options: SearchOptions,
    pub(super) panels_with_errors: HashMap<InputPanel, String>,
    pub(super) active_match_index: Option<usize>,
    pub(super) search_id: usize,
    pub(super) included_files_editor: Entity<Editor>,
    pub(super) excluded_files_editor: Entity<Editor>,
    pub(super) filters_enabled: bool,
    pub(super) replace_enabled: bool,
    pub(super) pending_replace_all: bool,
    pub(super) included_opened_only: bool,
    pub(super) regex_language: Option<Arc<Language>>,
    pub(super) _subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone)]
pub struct ProjectSearchSettings {
    pub(super) search_options: SearchOptions,
    pub(super) filters_enabled: bool,
}

pub struct ProjectSearchBar {
    pub(super) active_project_search: Option<Entity<ProjectSearchView>>,
    pub(super) subscription: Option<Subscription>,
}
