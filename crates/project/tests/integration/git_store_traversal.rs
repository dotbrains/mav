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

#[gpui::test]
async fn test_git_traversal_with_one_repo(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "x": {
                ".git": {},
                "x1.txt": "foo",
                "x2.txt": "bar",
                "y": {
                    ".git": {},
                    "y1.txt": "baz",
                    "y2.txt": "qux"
                },
                "z.txt": "sneaky..."
            },
            "z": {
                ".git": {},
                "z1.txt": "quux",
                "z2.txt": "quuux"
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/x/.git")),
        &[
            ("x2.txt", StatusCode::Modified.index()),
            ("z.txt", StatusCode::Added.index()),
        ],
    );
    fs.set_status_for_repo(Path::new(path!("/root/x/y/.git")), &[("y1.txt", CONFLICT)]);
    fs.set_status_for_repo(
        Path::new(path!("/root/z/.git")),
        &[("z2.txt", StatusCode::Added.index())],
    );

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let (repo_snapshots, worktree_snapshot) = project.read_with(cx, |project, cx| {
        (
            project.git_store().read(cx).repo_snapshots(cx),
            project.worktrees(cx).next().unwrap().read(cx).snapshot(),
        )
    });

    let traversal = GitTraversal::new(
        &repo_snapshots,
        worktree_snapshot.traverse_from_path(true, false, true, RelPath::unix("x").unwrap()),
    );
    let entries = traversal
        .map(|entry| (entry.path.clone(), entry.git_summary))
        .collect::<Vec<_>>();
    pretty_assertions::assert_eq!(
        entries,
        [
            (rel_path("x/x1.txt").into(), GitSummary::UNCHANGED),
            (rel_path("x/x2.txt").into(), MODIFIED),
            (rel_path("x/y/y1.txt").into(), GitSummary::CONFLICT),
            (rel_path("x/y/y2.txt").into(), GitSummary::UNCHANGED),
            (rel_path("x/z.txt").into(), ADDED),
            (rel_path("z/z1.txt").into(), GitSummary::UNCHANGED),
            (rel_path("z/z2.txt").into(), ADDED),
        ]
    )
}

#[gpui::test]
async fn test_git_traversal_with_nested_repos(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "x": {
                ".git": {},
                "x1.txt": "foo",
                "x2.txt": "bar",
                "y": {
                    ".git": {},
                    "y1.txt": "baz",
                    "y2.txt": "qux"
                },
                "z.txt": "sneaky..."
            },
            "z": {
                ".git": {},
                "z1.txt": "quux",
                "z2.txt": "quuux"
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/x/.git")),
        &[
            ("x2.txt", StatusCode::Modified.index()),
            ("z.txt", StatusCode::Added.index()),
        ],
    );
    fs.set_status_for_repo(Path::new(path!("/root/x/y/.git")), &[("y1.txt", CONFLICT)]);

    fs.set_status_for_repo(
        Path::new(path!("/root/z/.git")),
        &[("z2.txt", StatusCode::Added.index())],
    );

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let (repo_snapshots, worktree_snapshot) = project.read_with(cx, |project, cx| {
        (
            project.git_store().read(cx).repo_snapshots(cx),
            project.worktrees(cx).next().unwrap().read(cx).snapshot(),
        )
    });

    // Sanity check the propagation for x/y and z
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("x/y", GitSummary::CONFLICT),
            ("x/y/y1.txt", GitSummary::CONFLICT),
            ("x/y/y2.txt", GitSummary::UNCHANGED),
        ],
    );
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("z", ADDED),
            ("z/z1.txt", GitSummary::UNCHANGED),
            ("z/z2.txt", ADDED),
        ],
    );

    // Test one of the fundamental cases of propagation blocking, the transition from one git repository to another
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("x", MODIFIED + ADDED),
            ("x/y", GitSummary::CONFLICT),
            ("x/y/y1.txt", GitSummary::CONFLICT),
        ],
    );

    // Sanity check everything around it
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("x", MODIFIED + ADDED),
            ("x/x1.txt", GitSummary::UNCHANGED),
            ("x/x2.txt", MODIFIED),
            ("x/y", GitSummary::CONFLICT),
            ("x/y/y1.txt", GitSummary::CONFLICT),
            ("x/y/y2.txt", GitSummary::UNCHANGED),
            ("x/z.txt", ADDED),
        ],
    );

    // Test the other fundamental case, transitioning from git repository to non-git repository
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("", GitSummary::UNCHANGED),
            ("x", MODIFIED + ADDED),
            ("x/x1.txt", GitSummary::UNCHANGED),
        ],
    );

    // And all together now
    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("", GitSummary::UNCHANGED),
            ("x", MODIFIED + ADDED),
            ("x/x1.txt", GitSummary::UNCHANGED),
            ("x/x2.txt", MODIFIED),
            ("x/y", GitSummary::CONFLICT),
            ("x/y/y1.txt", GitSummary::CONFLICT),
            ("x/y/y2.txt", GitSummary::UNCHANGED),
            ("x/z.txt", ADDED),
            ("z", ADDED),
            ("z/z1.txt", GitSummary::UNCHANGED),
            ("z/z2.txt", ADDED),
        ],
    );
}

#[gpui::test]
async fn test_git_traversal_simple(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            "a": {
                "b": {
                    "c1.txt": "",
                    "c2.txt": "",
                },
                "d": {
                    "e1.txt": "",
                    "e2.txt": "",
                    "e3.txt": "",
                }
            },
            "f": {
                "no-status.txt": ""
            },
            "g": {
                "h1.txt": "",
                "h2.txt": ""
            },
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/.git")),
        &[
            ("a/b/c1.txt", StatusCode::Added.index()),
            ("a/d/e2.txt", StatusCode::Modified.index()),
            ("g/h2.txt", CONFLICT),
        ],
    );

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    cx.executor().run_until_parked();

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
            ("", GitSummary::CONFLICT + MODIFIED + ADDED),
            ("g", GitSummary::CONFLICT),
            ("g/h2.txt", GitSummary::CONFLICT),
        ],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("", GitSummary::CONFLICT + ADDED + MODIFIED),
            ("a", ADDED + MODIFIED),
            ("a/b", ADDED),
            ("a/b/c1.txt", ADDED),
            ("a/b/c2.txt", GitSummary::UNCHANGED),
            ("a/d", MODIFIED),
            ("a/d/e2.txt", MODIFIED),
            ("f", GitSummary::UNCHANGED),
            ("f/no-status.txt", GitSummary::UNCHANGED),
            ("g", GitSummary::CONFLICT),
            ("g/h2.txt", GitSummary::CONFLICT),
        ],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("a/b", ADDED),
            ("a/b/c1.txt", ADDED),
            ("a/b/c2.txt", GitSummary::UNCHANGED),
            ("a/d", MODIFIED),
            ("a/d/e1.txt", GitSummary::UNCHANGED),
            ("a/d/e2.txt", MODIFIED),
            ("f", GitSummary::UNCHANGED),
            ("f/no-status.txt", GitSummary::UNCHANGED),
            ("g", GitSummary::CONFLICT),
        ],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("a/b/c1.txt", ADDED),
            ("a/b/c2.txt", GitSummary::UNCHANGED),
            ("a/d/e1.txt", GitSummary::UNCHANGED),
            ("a/d/e2.txt", MODIFIED),
            ("f/no-status.txt", GitSummary::UNCHANGED),
        ],
    );
}

#[gpui::test]
async fn test_git_traversal_with_repos_under_project(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/root"),
        json!({
            "x": {
                ".git": {},
                "x1.txt": "foo",
                "x2.txt": "bar"
            },
            "y": {
                ".git": {},
                "y1.txt": "baz",
                "y2.txt": "qux"
            },
            "z": {
                ".git": {},
                "z1.txt": "quux",
                "z2.txt": "quuux"
            }
        }),
    )
    .await;

    fs.set_status_for_repo(
        Path::new(path!("/root/x/.git")),
        &[("x1.txt", StatusCode::Added.index())],
    );
    fs.set_status_for_repo(
        Path::new(path!("/root/y/.git")),
        &[
            ("y1.txt", CONFLICT),
            ("y2.txt", StatusCode::Modified.index()),
        ],
    );
    fs.set_status_for_repo(
        Path::new(path!("/root/z/.git")),
        &[("z2.txt", StatusCode::Modified.index())],
    );

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let (repo_snapshots, worktree_snapshot) = project.read_with(cx, |project, cx| {
        (
            project.git_store().read(cx).repo_snapshots(cx),
            project.worktrees(cx).next().unwrap().read(cx).snapshot(),
        )
    });

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[("x", ADDED), ("x/x1.txt", ADDED)],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("y", GitSummary::CONFLICT + MODIFIED),
            ("y/y1.txt", GitSummary::CONFLICT),
            ("y/y2.txt", MODIFIED),
        ],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[("z", MODIFIED), ("z/z2.txt", MODIFIED)],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[("x", ADDED), ("x/x1.txt", ADDED)],
    );

    check_git_statuses(
        &repo_snapshots,
        &worktree_snapshot,
        &[
            ("x", ADDED),
            ("x/x1.txt", ADDED),
            ("x/x2.txt", GitSummary::UNCHANGED),
            ("y", GitSummary::CONFLICT + MODIFIED),
            ("y/y1.txt", GitSummary::CONFLICT),
            ("y/y2.txt", MODIFIED),
            ("z", MODIFIED),
            ("z/z1.txt", GitSummary::UNCHANGED),
            ("z/z2.txt", MODIFIED),
        ],
    );
}

fn init_test(cx: &mut gpui::TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

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
