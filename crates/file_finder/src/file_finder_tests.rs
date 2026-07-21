use std::{future::IntoFuture, path::Path, time::Duration};

use super::*;
use editor::Editor;
use gpui::{Entity, TestAppContext, VisualTestContext};
use menu::{Cancel, Confirm, SelectNext, SelectPrevious};
use pretty_assertions::assert_matches;
use project::{FS_WATCH_LATENCY, RemoveOptions};
use serde_json::json;
use settings::SettingsStore;
use util::{path, rel_path::rel_path};
use workspace::{
    AppState, CloseActiveItem, Item, MultiWorkspace, OpenOptions, ToggleFileFinder, Workspace,
    open_paths,
};

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod core_unit;
mod history_core;
mod history_external;
mod history_order;
mod history_selection;
mod ignored_roots;
mod modifier_open;
mod path_matching;
mod query_ranges;
mod ranking;
mod refresh_selection;
mod refresh_updates;
mod worktrees_create;

async fn open_close_queried_buffer(
    input: &str,
    expected_matches: usize,
    expected_editor_title: &str,
    workspace: &Entity<Workspace>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<FoundPath> {
    let history_items = open_queried_buffer(
        input,
        expected_matches,
        expected_editor_title,
        workspace,
        cx,
    )
    .await;

    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });

    history_items
}

async fn open_queried_buffer(
    input: &str,
    expected_matches: usize,
    expected_editor_title: &str,
    workspace: &Entity<Workspace>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<FoundPath> {
    let picker = open_file_picker(workspace, cx);
    simulate_input(cx, input);

    let history_items = picker.update(cx, |finder, _| {
        assert_eq!(
            finder.delegate.matches.len(),
            expected_matches + 1, // +1 from CreateNew option
            "Unexpected number of matches found for query `{input}`, matches: {:?}",
            finder.delegate.matches
        );
        finder.delegate.history_items.clone()
    });

    cx.dispatch_action(Confirm);
    // Opening the buffer can trigger worktree updates that schedule a debounced
    // refresh; advance past it so a deferred confirm (confirm_on_update) runs.
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();

    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        let active_editor_title = active_editor.read(cx).title(cx);
        assert_eq!(
            expected_editor_title, active_editor_title,
            "Unexpected editor title for query `{input}`"
        );
    });

    history_items
}

fn init_test(cx: &mut TestAppContext) -> Arc<AppState> {
    cx.update(|cx| {
        let state = AppState::test(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        super::init(cx);
        editor::init(cx);
        state
    })
}

fn test_path_position(test_str: &str) -> FileSearchQuery {
    parse_file_search_query(test_str)
}

fn build_find_picker(
    project: Entity<Project>,
    cx: &mut TestAppContext,
) -> (
    Entity<Picker<FileFinderDelegate>>,
    Entity<Workspace>,
    &mut VisualTestContext,
) {
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let picker = open_file_picker(&workspace, cx);
    (picker, workspace, cx)
}

#[track_caller]
fn open_file_picker(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<FileFinderDelegate>> {
    cx.dispatch_action(ToggleFileFinder {
        separate_history: true,
    });
    active_file_picker(workspace, cx)
}

/// Type `input` into the file finder and then let the debounced search run.
///
/// `update_matches` delays the actual search by [`SEARCH_DEBOUNCE`] (see its
/// doc comment), and `run_until_parked` does not advance the clock, so tests
/// must move time forward for the search to execute.
fn simulate_input(cx: &mut VisualTestContext, input: &str) {
    cx.simulate_input(input);
    cx.executor().advance_clock(SEARCH_DEBOUNCE);
    cx.run_until_parked();
}

#[track_caller]
fn active_file_picker(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<Picker<FileFinderDelegate>> {
    workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<FileFinder>(cx)
            .expect("file finder is not open")
            .read(cx)
            .picker
            .clone()
    })
}

#[derive(Debug, Default)]
struct SearchEntries {
    history: Vec<Arc<RelPath>>,
    history_found_paths: Vec<FoundPath>,
    search: Vec<Arc<RelPath>>,
    search_matches: Vec<PathMatch>,
}

impl SearchEntries {
    #[track_caller]
    fn search_paths_only(self) -> Vec<Arc<RelPath>> {
        assert!(
            self.history.is_empty(),
            "Should have no history matches, but got: {:?}",
            self.history
        );
        self.search
    }

    #[track_caller]
    fn search_matches_only(self) -> Vec<PathMatch> {
        assert!(
            self.history.is_empty(),
            "Should have no history matches, but got: {:?}",
            self.history
        );
        self.search_matches
    }
}

fn collect_search_matches(picker: &Picker<FileFinderDelegate>) -> SearchEntries {
    let mut search_entries = SearchEntries::default();
    for m in &picker.delegate.matches.matches {
        match m {
            Match::History {
                path: history_path,
                panel_match: path_match,
            } => {
                if let Some(path_match) = path_match.as_ref() {
                    search_entries
                        .history
                        .push(path_match.0.path_prefix.join(&path_match.0.path));
                } else {
                    // This occurs when the query is empty and we show history matches
                    // that are outside the project.
                    panic!("currently not exercised in tests");
                }
                search_entries
                    .history_found_paths
                    .push(history_path.clone());
            }
            Match::Search(path_match) => {
                search_entries
                    .search
                    .push(path_match.0.path_prefix.join(&path_match.0.path));
                search_entries.search_matches.push(path_match.0.clone());
            }
            Match::CreateNew(_) => {}
            Match::Channel { .. } => {}
        }
    }
    search_entries
}

#[track_caller]
fn assert_match_selection(
    finder: &Picker<FileFinderDelegate>,
    expected_selection_index: usize,
    expected_file_name: &str,
) {
    assert_eq!(
        finder.delegate.selected_index(),
        expected_selection_index,
        "Match is not selected"
    );
    assert_match_at_position(finder, expected_selection_index, expected_file_name);
}

#[track_caller]
fn assert_match_at_position(
    finder: &Picker<FileFinderDelegate>,
    match_index: usize,
    expected_file_name: &str,
) {
    let match_item = finder
        .delegate
        .matches
        .get(match_index)
        .unwrap_or_else(|| panic!("Finder has no match for index {match_index}"));
    let match_file_name = match &match_item {
        Match::History { path, .. } => path.absolute.file_name().and_then(|s| s.to_str()),
        Match::Search(path_match) => path_match.0.path.file_name(),
        Match::CreateNew(project_path) => project_path.path.file_name(),
        Match::Channel { channel_name, .. } => Some(channel_name.as_str()),
    }
    .unwrap();
    assert_eq!(match_file_name, expected_file_name);
}
