use std::{path::Path, time::Duration};

use collections::HashMap;
use project::{
    Project,
    git_store::{RepositoryId, RepositorySnapshot},
};

use fs::FakeFs;
use git::status::{
    FileStatus, GitSummary, StatusCode, TrackedSummary, UnmergedStatus, UnmergedStatusCode,
};
use gpui::TestAppContext;
use project::GitTraversal;

use serde_json::json;
use settings::SettingsStore;
use util::{
    path,
    rel_path::{RelPath, rel_path},
};

const CONFLICT: FileStatus = FileStatus::Unmerged(UnmergedStatus {
    first_head: UnmergedStatusCode::Updated,
    second_head: UnmergedStatusCode::Updated,
});
const ADDED: GitSummary = GitSummary {
    index: TrackedSummary::ADDED,
    count: 1,
    ..GitSummary::UNCHANGED
};
const MODIFIED: GitSummary = GitSummary {
    index: TrackedSummary::MODIFIED,
    count: 1,
    ..GitSummary::UNCHANGED
};

fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[gpui::test]
async fn test_bump_mtime_of_git_repo_workdir(cx: &mut TestAppContext) {
    init_test(cx);

    // Create a worktree with a git directory.
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            "a.txt": "",
            "b": {
                "c.txt": "",
            },
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/root/.git").as_ref(),
        &[("a.txt", "".into()), ("b/c.txt", "".into())],
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let (old_entry_ids, old_mtimes) = project.read_with(cx, |project, cx| {
        let tree = project.worktrees(cx).next().unwrap().read(cx);
        (
            tree.entries(true, 0).map(|e| e.id).collect::<Vec<_>>(),
            tree.entries(true, 0).map(|e| e.mtime).collect::<Vec<_>>(),
        )
    });

    // Regression test: after the directory is scanned, touch the git repo's
    // working directory, bumping its mtime. That directory keeps its project
    // entry id after the directories are re-scanned.
    fs.touch_path(path!("/root")).await;
    cx.executor().run_until_parked();

    let (new_entry_ids, new_mtimes) = project.read_with(cx, |project, cx| {
        let tree = project.worktrees(cx).next().unwrap().read(cx);
        (
            tree.entries(true, 0).map(|e| e.id).collect::<Vec<_>>(),
            tree.entries(true, 0).map(|e| e.mtime).collect::<Vec<_>>(),
        )
    });
    assert_eq!(new_entry_ids, old_entry_ids);
    assert_ne!(new_mtimes, old_mtimes);

    // Regression test: changes to the git repository should still be
    // detected.
    fs.set_head_for_repo(
        path!("/root/.git").as_ref(),
        &[("a.txt", "".into()), ("b/c.txt", "something-else".into())],
        "deadbeef",
    );
    cx.executor().run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));

    let (repo_snapshots, worktree_snapshot) = project.read_with(cx, |project, cx| {
        (
            project.git_store().read(cx).repo_snapshots(cx),
            project.worktrees(cx).next().unwrap().read(cx).snapshot(),
        )
    });

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("", MODIFIED),
            ("a.txt", GitSummary::UNCHANGED),
            ("b/c.txt", MODIFIED),
        ],
    );
}

#[track_caller]
fn check_git_statuses(
    repo_snapshots: &HashMap<RepositoryId, RepositorySnapshot>,
    worktree_snapshot: &worktree::Snapshot,
    expected_statuses: &[(&str, GitSummary)],
) {
    let mut traversal = GitTraversal::new(
        repo_snapshots,
        worktree_snapshot.traverse_from_path(true, true, false, RelPath::empty()),
    );
    let found_statuses = expected_statuses
        .iter()
        .map(|&(path, _)| {
            let git_entry = traversal
                .find(|git_entry| git_entry.path.as_ref() == rel_path(path))
                .unwrap_or_else(|| panic!("Traversal has no entry for {path:?}"));
            (path, git_entry.git_summary)
        })
        .collect::<Vec<_>>();
    pretty_assertions::assert_eq!(found_statuses, expected_statuses);
}
