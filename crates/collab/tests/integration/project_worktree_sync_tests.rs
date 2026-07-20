use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use fs::Fs as _;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::SettingsStore;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_fs_operations(
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
            path!("/dir"),
            json!({
                "a.txt": "a-contents",
                "b.txt": "b-contents",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let worktree_a = project_a.read_with(cx_a, |project, cx| project.worktrees(cx).next().unwrap());
    let worktree_b = project_b.read_with(cx_b, |project, cx| project.worktrees(cx).next().unwrap());

    let entry = project_b
        .update(cx_b, |project, cx| {
            project.create_entry((worktree_id, rel_path("c.txt")), false, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [rel_path("a.txt"), rel_path("b.txt"), rel_path("c.txt")]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [rel_path("a.txt"), rel_path("b.txt"), rel_path("c.txt")]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            project.rename_entry(entry.id, (worktree_id, rel_path("d.txt")).into(), cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [rel_path("a.txt"), rel_path("b.txt"), rel_path("d.txt")]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "d.txt"]
        );
    });

    let dir_entry = project_b
        .update(cx_b, |project, cx| {
            project.create_entry((worktree_id, rel_path("DIR")), true, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["DIR", "a.txt", "b.txt", "d.txt"]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["DIR", "a.txt", "b.txt", "d.txt"]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            project.create_entry((worktree_id, rel_path("DIR/e.txt")), false, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    project_b
        .update(cx_b, |project, cx| {
            project.create_entry((worktree_id, rel_path("DIR/SUBDIR")), true, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    project_b
        .update(cx_b, |project, cx| {
            project.create_entry((worktree_id, rel_path("DIR/SUBDIR/f.txt")), false, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path("DIR"),
                rel_path("DIR/SUBDIR"),
                rel_path("DIR/SUBDIR/f.txt"),
                rel_path("DIR/e.txt"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("d.txt")
            ]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path("DIR"),
                rel_path("DIR/SUBDIR"),
                rel_path("DIR/SUBDIR/f.txt"),
                rel_path("DIR/e.txt"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("d.txt")
            ]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            project.copy_entry(
                entry.id,
                (worktree_b.read(cx).id(), rel_path("f.txt")).into(),
                cx,
            )
        })
        .await
        .unwrap()
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path("DIR"),
                rel_path("DIR/SUBDIR"),
                rel_path("DIR/SUBDIR/f.txt"),
                rel_path("DIR/e.txt"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("d.txt"),
                rel_path("f.txt")
            ]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree.paths().collect::<Vec<_>>(),
            [
                rel_path("DIR"),
                rel_path("DIR/SUBDIR"),
                rel_path("DIR/SUBDIR/f.txt"),
                rel_path("DIR/e.txt"),
                rel_path("a.txt"),
                rel_path("b.txt"),
                rel_path("d.txt"),
                rel_path("f.txt")
            ]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            project.delete_entry(dir_entry.id, false, cx).unwrap()
        })
        .await
        .unwrap();
    executor.run_until_parked();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "d.txt", "f.txt"]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "d.txt", "f.txt"]
        );
    });

    project_b
        .update(cx_b, |project, cx| {
            project.delete_entry(entry.id, false, cx).unwrap()
        })
        .await
        .unwrap();

    worktree_a.read_with(cx_a, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "f.txt"]
        );
    });

    worktree_b.read_with(cx_b, |worktree, _| {
        assert_eq!(
            worktree
                .paths()
                .map(|p| p.as_unix_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "f.txt"]
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_local_settings(
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

    // As client A, open a project that contains some local settings files
    client_a
        .fs()
        .insert_tree(
            "/dir",
            json!({
                ".mav": {
                    "settings.json": r#"{ "tab_size": 2 }"#
                },
                "a": {
                    ".mav": {
                        "settings.json": r#"{ "tab_size": 8 }"#
                    },
                    "a.txt": "a-contents",
                },
                "b": {
                    "b.txt": "b-contents",
                }
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project("/dir", cx_a).await;
    executor.run_until_parked();
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // As client B, join that project and observe the local settings.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let worktree_b = project_b.read_with(cx_b, |project, cx| project.worktrees(cx).next().unwrap());
    executor.run_until_parked();
    cx_b.read(|cx| {
        let store = cx.global::<SettingsStore>();
        assert_eq!(
            store
                .local_settings(worktree_b.read(cx).id())
                .map(|(path, content)| (
                    path,
                    content.all_languages.defaults.tab_size.map(Into::into)
                ))
                .collect::<Vec<_>>(),
            &[
                (rel_path("").into(), Some(2)),
                (rel_path("a").into(), Some(8)),
            ]
        )
    });

    // As client A, update a settings file. As Client B, see the changed settings.
    client_a
        .fs()
        .insert_file("/dir/.mav/settings.json", r#"{}"#.into())
        .await;
    executor.run_until_parked();
    cx_b.read(|cx| {
        let store = cx.global::<SettingsStore>();
        assert_eq!(
            store
                .local_settings(worktree_b.read(cx).id())
                .map(|(path, content)| (
                    path,
                    content.all_languages.defaults.tab_size.map(Into::into)
                ))
                .collect::<Vec<_>>(),
            &[(rel_path("").into(), None), (rel_path("a").into(), Some(8)),]
        )
    });

    // As client A, create and remove some settings files. As client B, see the changed settings.
    client_a
        .fs()
        .remove_file("/dir/.mav/settings.json".as_ref(), Default::default())
        .await
        .unwrap();
    client_a
        .fs()
        .create_dir("/dir/b/.mav".as_ref())
        .await
        .unwrap();
    client_a
        .fs()
        .insert_file("/dir/b/.mav/settings.json", r#"{"tab_size": 4}"#.into())
        .await;
    executor.run_until_parked();
    cx_b.read(|cx| {
        let store = cx.global::<SettingsStore>();
        assert_eq!(
            store
                .local_settings(worktree_b.read(cx).id())
                .map(|(path, content)| (
                    path,
                    content.all_languages.defaults.tab_size.map(Into::into)
                ))
                .collect::<Vec<_>>(),
            &[
                (rel_path("a").into(), Some(8)),
                (rel_path("b").into(), Some(4)),
            ]
        )
    });

    // As client B, disconnect.
    server.forbid_connections();
    server.disconnect_client(client_b.peer_id().unwrap());

    // As client A, change and remove settings files while client B is disconnected.
    client_a
        .fs()
        .insert_file("/dir/a/.mav/settings.json", r#"{"hard_tabs":true}"#.into())
        .await;
    client_a
        .fs()
        .remove_file("/dir/b/.mav/settings.json".as_ref(), Default::default())
        .await
        .unwrap();
    executor.run_until_parked();

    // As client B, reconnect and see the changed settings.
    server.allow_connections();
    executor.advance_clock(RECEIVE_TIMEOUT);
    cx_b.read(|cx| {
        let store = cx.global::<SettingsStore>();
        assert_eq!(
            store
                .local_settings(worktree_b.read(cx).id())
                .map(|(path, content)| (path, content.all_languages.defaults.hard_tabs))
                .collect::<Vec<_>>(),
            &[(rel_path("a").into(), Some(true))],
        )
    });
}
