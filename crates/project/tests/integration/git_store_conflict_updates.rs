use std::sync::mpsc;

use crate::Project;

use fs::FakeFs;
use git::{
    repository::{RepoPath, repo_path},
    status::{UnmergedStatus, UnmergedStatusCode},
};
use gpui::{BackgroundExecutor, TestAppContext};
use project::git_store::*;
use serde_json::json;
use text::{OffsetRangeExt, Point};
use unindent::Unindent as _;
use util::{path, rel_path::rel_path};

#[gpui::test]
async fn test_conflict_updates(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    zlog::init_test();
    cx.update(|cx| {
        settings::init(cx);
    });
    let initial_text = "
            one
            two
            three
            four
            five
        "
    .unindent();
    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "a.txt": initial_text,
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (git_store, buffer) = project.update(cx, |project, cx| {
        (
            project.git_store().clone(),
            project.open_local_buffer(path!("/project/a.txt"), cx),
        )
    });
    let buffer = buffer.await.unwrap();
    let conflict_set = git_store
        .update(cx, |git_store, cx| {
            git_store.open_conflict_set(buffer.clone(), cx)
        })
        .await;
    let (events_tx, events_rx) = mpsc::channel::<ConflictSetUpdate>();
    let _conflict_set_subscription = cx.update(|cx| {
        cx.subscribe(&conflict_set, move |_, event, _| {
            events_tx.send(event.clone()).ok();
        })
    });
    let conflicts_snapshot = conflict_set.read_with(cx, |conflict_set, _| conflict_set.snapshot());
    assert!(conflicts_snapshot.conflicts.is_empty());

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (4..4, "<<<<<<< HEAD\n"),
                (14..14, "=======\nTWO\n>>>>>>> branch\n"),
            ],
            None,
            cx,
        );
    });

    cx.run_until_parked();
    events_rx
        .try_recv()
        .expect_err("no conflicts should be registered as long as the file's status is unchanged");

    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.insert(
            repo_path("a.txt"),
            UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            },
        );
        // Cause the repository to update cached conflicts
        state.refs.insert("MERGE_HEAD".into(), "123".into())
    })
    .unwrap();

    cx.run_until_parked();
    let update = events_rx
        .try_recv()
        .expect("status change should trigger conflict parsing");
    assert_eq!(update.old_range, 0..0);
    assert_eq!(update.new_range, 0..1);

    let conflict = conflict_set.read_with(cx, |conflict_set, _| {
        conflict_set.snapshot().conflicts[0].clone()
    });
    cx.update(|cx| {
        conflict.resolve(buffer.clone(), std::slice::from_ref(&conflict.theirs), cx);
    });

    cx.run_until_parked();
    let update = events_rx
        .try_recv()
        .expect("conflicts should be removed after resolution");
    assert_eq!(update.old_range, 0..1);
    assert_eq!(update.new_range, 0..0);
}

#[gpui::test]
async fn test_conflict_updates_without_merge_head(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    zlog::init_test();
    cx.update(|cx| {
        settings::init(cx);
    });

    let initial_text = "
            zero
            <<<<<<< HEAD
            one
            =======
            two
            >>>>>>> Stashed Changes
            three
        "
    .unindent();

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "a.txt": initial_text,
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (git_store, buffer) = project.update(cx, |project, cx| {
        (
            project.git_store().clone(),
            project.open_local_buffer(path!("/project/a.txt"), cx),
        )
    });

    cx.run_until_parked();
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.insert(
            RepoPath::from_rel_path(rel_path("a.txt")),
            UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            },
        )
    })
    .unwrap();

    let buffer = buffer.await.unwrap();

    // Open the conflict set for a file that currently has conflicts.
    let conflict_set = git_store
        .update(cx, |git_store, cx| {
            git_store.open_conflict_set(buffer.clone(), cx)
        })
        .await;

    cx.run_until_parked();
    conflict_set.update(cx, |conflict_set, cx| {
        let conflict_range = conflict_set.snapshot().conflicts[0]
            .range
            .to_point(buffer.read(cx));
        assert_eq!(conflict_range, Point::new(1, 0)..Point::new(6, 0));
    });

    // Simulate the conflict being removed by e.g. staging the file.
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.remove(&repo_path("a.txt"))
    })
    .unwrap();

    cx.run_until_parked();
    conflict_set.update(cx, |conflict_set, _| {
        assert!(!conflict_set.has_conflict);
        assert_eq!(conflict_set.snapshot.conflicts.len(), 0);
    });

    // Simulate the conflict being re-added.
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.insert(
            repo_path("a.txt"),
            UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            },
        )
    })
    .unwrap();

    cx.run_until_parked();
    conflict_set.update(cx, |conflict_set, cx| {
        let conflict_range = conflict_set.snapshot().conflicts[0]
            .range
            .to_point(buffer.read(cx));
        assert_eq!(conflict_range, Point::new(1, 0)..Point::new(6, 0));
    });
}

#[gpui::test]
async fn test_conflict_updates_with_delayed_merge_head_conflicts(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    zlog::init_test();
    cx.update(|cx| {
        settings::init(cx);
    });

    let initial_text = "
            one
            two
            three
            four
        "
    .unindent();

    let conflicted_text = "
            one
            <<<<<<< HEAD
            two
            =======
            TWO
            >>>>>>> branch
            three
            four
        "
    .unindent();

    let resolved_text = "
            one
            TWO
            three
            four
        "
    .unindent();

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "a.txt": initial_text,
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let (git_store, buffer) = project.update(cx, |project, cx| {
        (
            project.git_store().clone(),
            project.open_local_buffer(path!("/project/a.txt"), cx),
        )
    });
    let buffer = buffer.await.unwrap();
    let conflict_set = git_store
        .update(cx, |git_store, cx| {
            git_store.open_conflict_set(buffer.clone(), cx)
        })
        .await;

    let (events_tx, events_rx) = mpsc::channel::<ConflictSetUpdate>();
    let _conflict_set_subscription = cx.update(|cx| {
        cx.subscribe(&conflict_set, move |_, event, _| {
            events_tx.send(event.clone()).ok();
        })
    });

    cx.run_until_parked();
    events_rx
        .try_recv()
        .expect_err("conflict set should start empty");

    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.refs.insert("MERGE_HEAD".into(), "123".into())
    })
    .unwrap();

    cx.run_until_parked();
    events_rx
        .try_recv()
        .expect_err("merge head without conflicted paths should not publish conflicts");
    conflict_set.update(cx, |conflict_set, _| {
        assert!(!conflict_set.has_conflict);
        assert_eq!(conflict_set.snapshot.conflicts.len(), 0);
    });

    buffer.update(cx, |buffer, cx| {
        buffer.set_text(conflicted_text.clone(), cx);
    });
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.insert(
            repo_path("a.txt"),
            UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            },
        );
    })
    .unwrap();

    cx.run_until_parked();
    let update = events_rx
        .try_recv()
        .expect("conflicts should appear once conflicted paths are visible");
    assert_eq!(update.old_range, 0..0);
    assert_eq!(update.new_range, 0..1);
    conflict_set.update(cx, |conflict_set, cx| {
        assert!(conflict_set.has_conflict);
        let conflict_range = conflict_set.snapshot().conflicts[0]
            .range
            .to_point(buffer.read(cx));
        assert_eq!(conflict_range, Point::new(1, 0)..Point::new(6, 0));
    });

    buffer.update(cx, |buffer, cx| {
        buffer.set_text(resolved_text.clone(), cx);
    });

    cx.run_until_parked();
    let update = events_rx
        .try_recv()
        .expect("resolved buffer text should clear visible conflict markers");
    assert_eq!(update.old_range, 0..1);
    assert_eq!(update.new_range, 0..0);
    conflict_set.update(cx, |conflict_set, _| {
        assert!(conflict_set.has_conflict);
        assert_eq!(conflict_set.snapshot.conflicts.len(), 0);
    });

    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.refs.insert("MERGE_HEAD".into(), "456".into());
    })
    .unwrap();

    cx.run_until_parked();
    events_rx.try_recv().expect_err(
        "merge-head change without unmerged-path changes should not emit marker updates",
    );
    conflict_set.update(cx, |conflict_set, _| {
        assert!(conflict_set.has_conflict);
        assert_eq!(conflict_set.snapshot.conflicts.len(), 0);
    });

    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.unmerged_paths.remove(&repo_path("a.txt"));
        state.refs.remove("MERGE_HEAD");
    })
    .unwrap();

    cx.run_until_parked();
    let update = events_rx
        .try_recv()
        .expect("status catch-up should emit a no-op update when clearing stale conflict state");
    assert_eq!(update.old_range, 0..0);
    assert_eq!(update.new_range, 0..0);
    assert!(update.buffer_range.is_none());
    conflict_set.update(cx, |conflict_set, _| {
        assert!(!conflict_set.has_conflict);
        assert_eq!(conflict_set.snapshot.conflicts.len(), 0);
    });
}
