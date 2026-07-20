use buffer_diff::{DiffHunkSecondaryStatus, DiffHunkStatus, assert_hunks};
use call::ActiveCall;
use collections::HashMap;
use fs::{Fs as _, RemoveOptions};
use git::{
    repository::repo_path,
    status::{FileStatus, StatusCode, TrackedStatus, UnmergedStatus, UnmergedStatusCode},
};
use gpui::{App, BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use project::Project;
use serde_json::json;
use std::path::Path;
use unindent::Unindent as _;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_git_diff_index_matches_head(
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

    let committed_text = "
        one
        two
        three
    "
    .unindent();
    let file_contents = "
        one
        TWO
        three
    "
    .unindent();

    client_a
        .fs()
        .insert_tree(
            "/dir",
            json!({
                ".git": {},
                "a.txt": file_contents,
            }),
        )
        .await;
    client_a
        .fs()
        .set_head_and_index_for_repo(Path::new("/dir/.git"), &[("a.txt", committed_text.clone())]);

    let (project_local, worktree_id) = client_a.build_local_project("/dir", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| {
            call.share_project(project_local.clone(), cx)
        })
        .await
        .unwrap();
    let project_remote = client_b.join_remote_project(project_id, cx_b).await;

    // Open the uncommitted diff on the guest, without opening it on the host
    // first, so that the host loads the diff bases in response to the guest's
    // request.
    let remote_buffer = project_remote
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("a.txt")), cx)
        })
        .await
        .unwrap();
    let remote_uncommitted_diff = project_remote
        .update(cx_b, |p, cx| {
            p.open_uncommitted_diff(remote_buffer.clone(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // The guest's index and head texts share one allocation, which is only
    // possible if the host detected that the index matches the head and sent
    // `Mode::IndexMatchesHead`.
    let buffer_id = remote_buffer.read_with(cx_b, |buffer, _| buffer.remote_id());
    project_remote.read_with(cx_b, |project, cx| {
        assert!(
            project
                .git_store()
                .read(cx)
                .index_matches_head_for_buffer(buffer_id, cx),
            "the host should send IndexMatchesHead when the index is clean"
        );
    });

    remote_uncommitted_diff.read_with(cx_b, |diff, cx| {
        let buffer = remote_buffer.read(cx);
        assert_eq!(
            diff.base_text_string(cx).as_deref(),
            Some(committed_text.as_str())
        );
        assert_hunks(
            diff.snapshot(cx).hunks_in_row_range(0..3, buffer),
            buffer,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(DiffHunkSecondaryStatus::HasSecondaryHunk),
            )],
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_git_branch_name(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            "/dir",
            json!({
            ".git": {},
            }),
        )
        .await;

    let (project_local, _worktree_id) = client_a.build_local_project("/dir", cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| {
            call.share_project(project_local.clone(), cx)
        })
        .await
        .unwrap();

    let project_remote = client_b.join_remote_project(project_id, cx_b).await;
    client_a
        .fs()
        .set_branch_name(Path::new("/dir/.git"), Some("branch-1"));

    // Wait for it to catch up to the new branch
    executor.run_until_parked();

    #[track_caller]
    fn assert_branch(branch_name: Option<impl Into<String>>, project: &Project, cx: &App) {
        let branch_name = branch_name.map(Into::into);
        let repositories = project.repositories(cx).values().collect::<Vec<_>>();
        assert_eq!(repositories.len(), 1);
        let repository = repositories[0].clone();
        assert_eq!(
            repository
                .read(cx)
                .branch
                .as_ref()
                .map(|branch| branch.name().to_owned()),
            branch_name
        )
    }

    // Smoke test branch reading

    project_local.read_with(cx_a, |project, cx| {
        assert_branch(Some("branch-1"), project, cx)
    });

    project_remote.read_with(cx_b, |project, cx| {
        assert_branch(Some("branch-1"), project, cx)
    });

    client_a
        .fs()
        .set_branch_name(Path::new("/dir/.git"), Some("branch-2"));

    // Wait for buffer_local_a to receive it
    executor.run_until_parked();

    // Smoke test branch reading

    project_local.read_with(cx_a, |project, cx| {
        assert_branch(Some("branch-2"), project, cx)
    });

    project_remote.read_with(cx_b, |project, cx| {
        assert_branch(Some("branch-2"), project, cx)
    });

    let project_remote_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    project_remote_c.read_with(cx_c, |project, cx| {
        assert_branch(Some("branch-2"), project, cx)
    });
}

#[gpui::test]
async fn test_git_status_sync(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                ".git": {},
                "a.txt": "a",
                "b.txt": "b",
                "c.txt": "c",
            }),
        )
        .await;

    // Initially, a.txt is uncommitted, but present in the index,
    // and b.txt is unmerged.
    client_a.fs().set_head_for_repo(
        path!("/dir/.git").as_ref(),
        &[("b.txt", "B".into()), ("c.txt", "c".into())],
        "deadbeef",
    );
    client_a.fs().set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[
            ("a.txt", "".into()),
            ("b.txt", "B".into()),
            ("c.txt", "c".into()),
        ],
    );
    client_a.fs().set_unmerged_paths_for_repo(
        path!("/dir/.git").as_ref(),
        &[(
            repo_path("b.txt"),
            UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Deleted,
            },
        )],
    );

    const A_STATUS_START: FileStatus = FileStatus::Tracked(TrackedStatus {
        index_status: StatusCode::Added,
        worktree_status: StatusCode::Modified,
    });
    const B_STATUS_START: FileStatus = FileStatus::Unmerged(UnmergedStatus {
        first_head: UnmergedStatusCode::Updated,
        second_head: UnmergedStatusCode::Deleted,
    });

    let (project_local, _worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| {
            call.share_project(project_local.clone(), cx)
        })
        .await
        .unwrap();

    let project_remote = client_b.join_remote_project(project_id, cx_b).await;

    // Wait for it to catch up to the new status
    executor.run_until_parked();

    #[track_caller]
    fn assert_status(file: &str, status: Option<FileStatus>, project: &Project, cx: &App) {
        let file = repo_path(file);
        let repos = project
            .repositories(cx)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(repos.len(), 1);
        let repo = repos.into_iter().next().unwrap();
        assert_eq!(
            repo.read(cx)
                .status_for_path(&file)
                .map(|entry| entry.status),
            status
        );
    }

    project_local.read_with(cx_a, |project, cx| {
        assert_status("a.txt", Some(A_STATUS_START), project, cx);
        assert_status("b.txt", Some(B_STATUS_START), project, cx);
        assert_status("c.txt", None, project, cx);
    });

    project_remote.read_with(cx_b, |project, cx| {
        assert_status("a.txt", Some(A_STATUS_START), project, cx);
        assert_status("b.txt", Some(B_STATUS_START), project, cx);
        assert_status("c.txt", None, project, cx);
    });

    const A_STATUS_END: FileStatus = FileStatus::Tracked(TrackedStatus {
        index_status: StatusCode::Added,
        worktree_status: StatusCode::Unmodified,
    });
    const B_STATUS_END: FileStatus = FileStatus::Tracked(TrackedStatus {
        index_status: StatusCode::Deleted,
        worktree_status: StatusCode::Added,
    });
    const C_STATUS_END: FileStatus = FileStatus::Tracked(TrackedStatus {
        index_status: StatusCode::Unmodified,
        worktree_status: StatusCode::Modified,
    });

    // Delete b.txt from the index, mark conflict as resolved,
    // and modify c.txt in the working copy.
    client_a.fs().set_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("a.txt", "a".into()), ("c.txt", "c".into())],
    );
    client_a
        .fs()
        .set_unmerged_paths_for_repo(path!("/dir/.git").as_ref(), &[]);
    client_a
        .fs()
        .atomic_write(path!("/dir/c.txt").into(), "CC".into())
        .await
        .unwrap();

    // Wait for buffer_local_a to receive it
    executor.run_until_parked();

    // Smoke test status reading
    project_local.read_with(cx_a, |project, cx| {
        assert_status("a.txt", Some(A_STATUS_END), project, cx);
        assert_status("b.txt", Some(B_STATUS_END), project, cx);
        assert_status("c.txt", Some(C_STATUS_END), project, cx);
    });

    project_remote.read_with(cx_b, |project, cx| {
        assert_status("a.txt", Some(A_STATUS_END), project, cx);
        assert_status("b.txt", Some(B_STATUS_END), project, cx);
        assert_status("c.txt", Some(C_STATUS_END), project, cx);
    });

    // And synchronization while joining
    let project_remote_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    project_remote_c.read_with(cx_c, |project, cx| {
        assert_status("a.txt", Some(A_STATUS_END), project, cx);
        assert_status("b.txt", Some(B_STATUS_END), project, cx);
        assert_status("c.txt", Some(C_STATUS_END), project, cx);
    });

    // Now remove the original git repository and check that collaborators are notified.
    client_a
        .fs()
        .remove_dir(path!("/dir/.git").as_ref(), RemoveOptions::default())
        .await
        .unwrap();

    executor.run_until_parked();
    project_remote.update(cx_b, |project, cx| {
        pretty_assertions::assert_eq!(
            project.git_store().read(cx).repo_snapshots(cx),
            HashMap::default()
        );
    });
    project_remote_c.update(cx_c, |project, cx| {
        pretty_assertions::assert_eq!(
            project.git_store().read(cx).repo_snapshots(cx),
            HashMap::default()
        );
    });
}
