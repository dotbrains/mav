mod worktree_settings_tests;

use anyhow::Result;
use encoding_rs;
use fs::{FakeFs, Fs, PathEventKind, RealFs, RemoveOptions};
use git::{DOT_GIT, GITIGNORE, REPO_EXCLUDE};
use gpui::{AppContext as _, BackgroundExecutor, BorrowAppContext, Context, Task, TestAppContext};
use parking_lot::Mutex;
use postage::stream::Stream;
use rand::prelude::*;
use rpc::{AnyProtoClient, NoopProtoClient, proto};
use worktree::{Entry, EntryKind, Event, PathChange, Worktree, WorktreeModelHandle};

use serde_json::json;
use settings::{SettingsStore, WorktreeId};
use std::{
    cell::Cell,
    env,
    fmt::Write,
    mem,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use util::{
    ResultExt, path,
    paths::PathStyle,
    rel_path::{RelPath, rel_path},
    test::TempTree,
};

mod basic_create_ops;
mod basic_ops;
mod deferred_watch;
mod encoding;
mod fs_events;
mod gitignore_excludes;
mod gitignored_dirs;
mod gitignored_files;
mod random_ops;
mod random_ops_helpers;
mod refresh_and_single_file;
mod remote_events;
mod repository_metadata;
mod repository_metadata_events;
mod rescan;
mod scan_filters;
mod traversal_symlink_mutation_tests;
mod traversal_symlink_real_fs_tests;
mod traversal_symlink_tests;

#[derive(Default)]
struct WorktreeExpectations {
    excluded_paths: &'static [&'static str],
    ignored_paths: &'static [&'static str],
    tracked_paths: &'static [&'static str],
    included_paths: &'static [&'static str],
}

#[track_caller]
fn check_worktree_entries(tree: &Worktree, expectations: WorktreeExpectations) {
    for path in expectations.excluded_paths {
        let entry = tree.entry_for_path(rel_path(path));
        assert!(
            entry.is_none(),
            "expected path '{path}' to be excluded, but got entry: {entry:?}",
        );
    }
    for path in expectations.ignored_paths {
        let entry = tree
            .entry_for_path(rel_path(path))
            .unwrap_or_else(|| panic!("Missing entry for expected ignored path '{path}'"));
        assert!(
            entry.is_ignored,
            "expected path '{path}' to be ignored, but got entry: {entry:?}",
        );
    }
    for path in expectations.tracked_paths {
        let entry = tree
            .entry_for_path(rel_path(path))
            .unwrap_or_else(|| panic!("Missing entry for expected tracked path '{path}'"));
        assert!(
            !entry.is_ignored || entry.is_always_included,
            "expected path '{path}' to be tracked, but got entry: {entry:?}",
        );
    }
    for path in expectations.included_paths {
        let entry = tree
            .entry_for_path(rel_path(path))
            .unwrap_or_else(|| panic!("Missing entry for expected included path '{path}'"));
        assert!(
            entry.is_always_included,
            "expected path '{path}' to always be included, but got entry: {entry:?}",
        );
    }
}

fn drain_git_repo_updates(events: &mut futures::channel::mpsc::UnboundedReceiver<Event>) -> bool {
    let mut found = false;
    while let Ok(event) = events.try_recv() {
        if matches!(event, Event::UpdatedGitRepositories(_)) {
            found = true;
        }
    }
    found
}

fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

async fn wait_for_condition(
    cx: &mut TestAppContext,
    mut condition: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..50 {
        if condition(cx) {
            return;
        }
        cx.executor().run_until_parked();
        cx.background_executor
            .timer(std::time::Duration::from_millis(10))
            .await;
    }
    panic!("timed out waiting for test condition");
}
