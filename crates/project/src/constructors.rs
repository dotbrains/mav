use super::*;

mod collab;
mod local;
mod remote;

const MAX_PROJECT_SEARCH_HISTORY_SIZE: usize = 500;

fn new_search_history() -> SearchHistory {
    SearchHistory::new(
        Some(MAX_PROJECT_SEARCH_HISTORY_SIZE),
        search_history::QueryInsertionBehavior::AlwaysInsert,
    )
}
