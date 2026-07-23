use super::*;
use fs::{FakeFs, Fs as _};
use git::repository::Worktree as GitWorktree;
use gpui::{BorrowAppContext, TestAppContext};
use project::Project;
use serde_json::json;
use settings::SettingsStore;
use std::time::Duration;
use workspace::MultiWorkspace;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        // Use an isolated DB so parallel tests can't see each other's
        // created-worktree records.
        cx.set_global(db::AppDatabase::test_new());
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
}

async fn fake_worktree_created_at(fs: &FakeFs, worktree_path: &Path) -> SystemTime {
    crate::test_support::fake_worktree_created_at(fs, worktree_path).await
}

async fn record_mav_created_worktree(fs: &FakeFs, worktree_path: &Path, cx: &mut TestAppContext) {
    crate::test_support::record_mav_created_worktree(fs, worktree_path, None, cx).await
}
