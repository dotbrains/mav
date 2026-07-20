use buffer_diff::{DiffHunkSecondaryStatus, DiffHunkStatus, assert_hunks};
use call::ActiveCall;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::Path;
use unindent::Unindent as _;
use util::rel_path::rel_path;

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_git_diff_base_change(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            "/dir",
            json!({
            ".git": {},
            "sub": {
                ".git": {},
                "b.txt": "
                    one
                    two
                    three
                ".unindent(),
            },
            "a.txt": "
                    one
                    two
                    three
                ".unindent(),
            }),
        )
        .await;

    let (project_local, worktree_id) = client_a.build_local_project("/dir", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| {
            call.share_project(project_local.clone(), cx)
        })
        .await
        .unwrap();

    let project_remote = client_b.join_remote_project(project_id, cx_b).await;

    let staged_text = "
        one
        three
    "
    .unindent();

    let committed_text = "
        one
        TWO
        three
    "
    .unindent();

    let new_committed_text = "
        one
        TWO_HUNDRED
        three
    "
    .unindent();

    let new_staged_text = "
        one
        two
    "
    .unindent();

    client_a
        .fs()
        .set_index_for_repo(Path::new("/dir/.git"), &[("a.txt", staged_text.clone())]);
    client_a.fs().set_head_for_repo(
        Path::new("/dir/.git"),
        &[("a.txt", committed_text.clone())],
        "deadbeef",
    );

    // Create the buffer
    let buffer_local_a = project_local
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let local_unstaged_diff_a = project_local
        .update(cx_a, |p, cx| {
            p.open_unstaged_diff(buffer_local_a.clone(), cx)
        })
        .await
        .unwrap();

    // Wait for it to catch up to the new diff
    executor.run_until_parked();
    local_unstaged_diff_a.read_with(cx_a, |diff, cx| {
        let buffer = buffer_local_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(1..2, "", "two\n", DiffHunkStatus::added_none())],
        );
    });

    // Create remote buffer
    let remote_buffer_a = project_remote
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let remote_unstaged_diff_a = project_remote
        .update(cx_b, |p, cx| {
            p.open_unstaged_diff(remote_buffer_a.clone(), cx)
        })
        .await
        .unwrap();

    // Wait remote buffer to catch up to the new diff
    executor.run_until_parked();
    remote_unstaged_diff_a.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(1..2, "", "two\n", DiffHunkStatus::added_none())],
        );
    });

    // Open uncommitted changes on the guest, without opening them on the host first
    let remote_uncommitted_diff_a = project_remote
        .update(cx_b, |p, cx| {
            p.open_uncommitted_diff(remote_buffer_a.clone(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    remote_uncommitted_diff_a.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(committed_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "TWO\n",
                "two\n",
                DiffHunkStatus::modified(DiffHunkSecondaryStatus::HasSecondaryHunk),
            )],
        );
    });

    // Update the index text of the open buffer
    client_a.fs().set_index_for_repo(
        Path::new("/dir/.git"),
        &[("a.txt", new_staged_text.clone())],
    );
    client_a.fs().set_head_for_repo(
        Path::new("/dir/.git"),
        &[("a.txt", new_committed_text.clone())],
        "deadbeef",
    );

    // Wait for buffer_local_a to receive it
    executor.run_until_parked();
    local_unstaged_diff_a.read_with(cx_a, |diff, cx| {
        let buffer = buffer_local_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(new_staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(2..3, "", "three\n", DiffHunkStatus::added_none())],
        );
    });

    // Guest receives index text update
    remote_unstaged_diff_a.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(new_staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(2..3, "", "three\n", DiffHunkStatus::added_none())],
        );
    });

    remote_uncommitted_diff_a.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_a.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(new_committed_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "TWO_HUNDRED\n",
                "two\n",
                DiffHunkStatus::modified(DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk),
            )],
        );
    });

    // Nested git dir
    let staged_text = "
        one
        three
    "
    .unindent();

    let new_staged_text = "
        one
        two
    "
    .unindent();

    client_a.fs().set_index_for_repo(
        Path::new("/dir/sub/.git"),
        &[("b.txt", staged_text.clone())],
    );

    // Create the buffer
    let buffer_local_b = project_local
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("sub/b.txt")), cx)
        })
        .await
        .unwrap();
    let local_unstaged_diff_b = project_local
        .update(cx_a, |p, cx| {
            p.open_unstaged_diff(buffer_local_b.clone(), cx)
        })
        .await
        .unwrap();

    // Wait for it to catch up to the new diff
    executor.run_until_parked();
    local_unstaged_diff_b.read_with(cx_a, |diff, cx| {
        let buffer = buffer_local_b.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(1..2, "", "two\n", DiffHunkStatus::added_none())],
        );
    });

    // Create remote buffer
    let remote_buffer_b = project_remote
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("sub/b.txt")), cx)
        })
        .await
        .unwrap();
    let remote_unstaged_diff_b = project_remote
        .update(cx_b, |p, cx| {
            p.open_unstaged_diff(remote_buffer_b.clone(), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    remote_unstaged_diff_b.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_b.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &staged_text,
            &[(1..2, "", "two\n", DiffHunkStatus::added_none())],
        );
    });

    // Updatet the staged text
    client_a.fs().set_index_for_repo(
        Path::new("/dir/sub/.git"),
        &[("b.txt", new_staged_text.clone())],
    );

    // Wait for buffer_local_b to receive it
    executor.run_until_parked();
    local_unstaged_diff_b.read_with(cx_a, |diff, cx| {
        let buffer = buffer_local_b.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(new_staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &new_staged_text,
            &[(2..3, "", "three\n", DiffHunkStatus::added_none())],
        );
    });

    remote_unstaged_diff_b.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer_b.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(new_staged_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..4, buffer),
            buffer,
            &new_staged_text,
            &[(2..3, "", "three\n", DiffHunkStatus::added_none())],
        );
    });
}
