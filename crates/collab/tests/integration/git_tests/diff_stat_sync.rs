use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_diff_stat_sync_between_host_and_downstream_client(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.background_executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;

    let fs = client_a.fs();
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "src": {
                    "lib.rs": "line1\nline2\nline3\n",
                    "new_file.rs": "added1\nadded2\n",
                },
                "README.md": "# project 1",
            }
        }),
    )
    .await;

    let dot_git = Path::new(path!("/code/project1/.git"));
    fs.set_head_for_repo(
        dot_git,
        &[
            ("src/lib.rs", "line1\nold_line2\n".into()),
            ("src/deleted.rs", "was_here\n".into()),
        ],
        "deadbeef",
    );
    fs.set_index_for_repo(
        dot_git,
        &[
            ("src/lib.rs", "line1\nold_line2\nline3\nline4\n".into()),
            ("src/staged_only.rs", "x\ny\n".into()),
            ("src/new_file.rs", "added1\nadded2\n".into()),
            ("README.md", "# project 1".into()),
        ],
    );

    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/code/project1"), cx_a)
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let _project_c = client_c.join_remote_project(project_id, cx_c).await;
    cx_a.run_until_parked();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let panel_a = workspace_a.update_in(cx_a, GitPanel::new_test);
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.add_panel(panel_a.clone(), window, cx);
    });

    let panel_b = workspace_b.update_in(cx_b, GitPanel::new_test);
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.add_panel(panel_b.clone(), window, cx);
    });

    cx_a.run_until_parked();

    let stats_a = collect_diff_stats(&panel_a, cx_a);
    let stats_b = collect_diff_stats(&panel_b, cx_b);

    let mut expected: HashMap<RepoPath, DiffStat> = HashMap::default();
    expected.insert(
        RepoPath::new("src/lib.rs").unwrap(),
        DiffStat {
            added: 3,
            deleted: 2,
        },
    );
    expected.insert(
        RepoPath::new("src/deleted.rs").unwrap(),
        DiffStat {
            added: 0,
            deleted: 1,
        },
    );
    expected.insert(
        RepoPath::new("src/new_file.rs").unwrap(),
        DiffStat {
            added: 2,
            deleted: 0,
        },
    );
    expected.insert(
        RepoPath::new("README.md").unwrap(),
        DiffStat {
            added: 1,
            deleted: 0,
        },
    );
    assert_eq!(stats_a, expected, "host diff stats should match expected");
    assert_eq!(stats_a, stats_b, "host and remote should agree");

    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();

    let _buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();
    cx_a.run_until_parked();

    buffer_a.update(cx_a, |buf, cx| {
        buf.edit([(buf.len()..buf.len(), "line4\n")], None, cx);
    });
    project_a
        .update(cx_a, |project, cx| {
            project.save_buffer(buffer_a.clone(), cx)
        })
        .await
        .unwrap();
    cx_a.run_until_parked();

    let stats_a = collect_diff_stats(&panel_a, cx_a);
    let stats_b = collect_diff_stats(&panel_b, cx_b);

    let mut expected_after_edit = expected.clone();
    expected_after_edit.insert(
        RepoPath::new("src/lib.rs").unwrap(),
        DiffStat {
            added: 4,
            deleted: 2,
        },
    );
    assert_eq!(
        stats_a, expected_after_edit,
        "host diff stats should reflect the edit"
    );
    assert_eq!(
        stats_b, expected_after_edit,
        "remote diff stats should reflect the host's edit"
    );

    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    cx_a.run_until_parked();

    let user_id_b = client_b.current_user_id(cx_b).to_proto();
    active_call_a
        .update(cx_a, |call, cx| call.invite(user_id_b, None, cx))
        .await
        .unwrap();
    cx_b.run_until_parked();
    let active_call_b = cx_b.read(ActiveCall::global);
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    cx_a.run_until_parked();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    cx_a.run_until_parked();

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let panel_b = workspace_b.update_in(cx_b, GitPanel::new_test);
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.add_panel(panel_b.clone(), window, cx);
    });
    cx_b.run_until_parked();

    let stats_b = collect_diff_stats(&panel_b, cx_b);
    assert_eq!(
        stats_b, expected_after_edit,
        "remote diff stats should be restored from the database after rejoining the call"
    );
}
