use crate::TestServer;
use call::ActiveCall;
use collections::HashSet;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use std::path::Path;
use util::path;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test]
async fn test_remote_git_branches(
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
        .insert_tree("/project", serde_json::json!({ ".git":{} }))
        .await;
    let branches = ["main", "dev", "feature-1"];
    client_a
        .fs()
        .insert_branches(Path::new("/project/.git"), &branches);
    let branches_set = branches
        .into_iter()
        .map(ToString::to_string)
        .collect::<HashSet<_>>();

    let (project_a, _) = client_a.build_local_project("/project", cx_a).await;

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Client A sees that a guest has joined and the repo has been populated
    executor.run_until_parked();

    let repo_b = cx_b.update(|cx| project_b.read(cx).active_repository(cx).unwrap());

    let branches_b = cx_b
        .update(|cx| repo_b.update(cx, |repository, _| repository.branches()))
        .await
        .unwrap()
        .unwrap();

    let new_branch = branches[2];

    let branches_b = branches_b
        .branches
        .into_iter()
        .map(|branch| branch.name().to_string())
        .collect::<HashSet<_>>();

    assert_eq!(branches_b, branches_set);

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.change_branch(new_branch.to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_branch = cx_a.update(|cx| {
        project_a.update(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .next()
                .unwrap()
                .read(cx)
                .branch
                .as_ref()
                .unwrap()
                .clone()
        })
    });

    assert_eq!(host_branch.name(), branches[2]);

    // Also try creating a new branch
    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.create_branch("totally-new-branch".to_string(), None)
        })
    })
    .await
    .unwrap()
    .unwrap();

    cx_b.update(|cx| {
        repo_b.update(cx, |repository, _cx| {
            repository.change_branch("totally-new-branch".to_string())
        })
    })
    .await
    .unwrap()
    .unwrap();

    executor.run_until_parked();

    let host_branch = cx_a.update(|cx| {
        project_a.update(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .next()
                .unwrap()
                .read(cx)
                .branch
                .as_ref()
                .unwrap()
                .clone()
        })
    });

    assert_eq!(host_branch.name(), "totally-new-branch");
}

#[gpui::test]
async fn test_guest_can_rejoin_shared_project_after_leaving_call(
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

    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({
                "file.txt": "hello\n",
            }),
        )
        .await;

    let (project_a, _worktree_id) = client_a.build_local_project(path!("/project"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let _project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    // third client joins call to prevent room from being torn down
    let _project_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();

    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let user_id_b = client_b.current_user_id(cx_b).to_proto();
    let active_call_a = cx_a.read(ActiveCall::global);
    active_call_a
        .update(cx_a, |call, cx| call.invite(user_id_b, None, cx))
        .await
        .unwrap();
    executor.run_until_parked();
    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let _project_b2 = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, _| {
        let guest_count = project
            .collaborators()
            .values()
            .filter(|c| !c.is_host)
            .count();

        assert_eq!(
            guest_count, 2,
            "host should have exactly one guest collaborator after rejoin"
        );
    });

    _project_b.read_with(cx_b, |project, _| {
        assert_eq!(
            project.client_subscriptions().len(),
            0,
            "We should clear all host subscriptions after leaving the project"
        );
    })
}
