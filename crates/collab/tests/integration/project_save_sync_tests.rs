use call::ActiveCall;
use fs::Fs as _;
use gpui::{BackgroundExecutor, TestAppContext};
use language::{Language, LanguageConfig, LanguageMatcher, tree_sitter_rust};
use pretty_assertions::assert_eq;
use project::ProjectPath;
use serde_json::json;
use std::sync::Arc;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_propagate_saves_and_fs_changes(
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

    let rust = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    let javascript = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["js".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    for client in [&client_a, &client_b, &client_c] {
        client.language_registry().add(rust.clone());
        client.language_registry().add(javascript.clone());
    }

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "file1.rs": "",
                "file2": ""
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;

    let worktree_a = project_a.read_with(cx_a, |p, cx| p.worktrees(cx).next().unwrap());
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // Join that worktree as clients B and C.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let project_c = client_c.join_remote_project(project_id, cx_c).await;

    let worktree_b = project_b.read_with(cx_b, |p, cx| p.worktrees(cx).next().unwrap());

    let worktree_c = project_c.read_with(cx_c, |p, cx| p.worktrees(cx).next().unwrap());

    // Open and edit a buffer as both guests B and C.
    let buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("file1.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_c = project_c
        .update(cx_c, |p, cx| {
            p.open_buffer((worktree_id, rel_path("file1.rs")), cx)
        })
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.language().unwrap().name(), "Rust");
    });

    buffer_c.read_with(cx_c, |buffer, _| {
        assert_eq!(buffer.language().unwrap().name(), "Rust");
    });
    buffer_b.update(cx_b, |buf, cx| buf.edit([(0..0, "i-am-b, ")], None, cx));
    buffer_c.update(cx_c, |buf, cx| buf.edit([(0..0, "i-am-c, ")], None, cx));

    // Open and edit that buffer as the host.
    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("file1.rs")), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buf, _| assert_eq!(buf.text(), "i-am-c, i-am-b, "));
    buffer_a.update(cx_a, |buf, cx| {
        buf.edit([(buf.len()..buf.len(), "i-am-a")], None, cx)
    });

    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buf, _| {
        assert_eq!(buf.text(), "i-am-c, i-am-b, i-am-a");
    });

    buffer_b.read_with(cx_b, |buf, _| {
        assert_eq!(buf.text(), "i-am-c, i-am-b, i-am-a");
    });

    buffer_c.read_with(cx_c, |buf, _| {
        assert_eq!(buf.text(), "i-am-c, i-am-b, i-am-a");
    });

    // Edit the buffer as the host and concurrently save as guest B.
    let save_b = project_b.update(cx_b, |project, cx| {
        project.save_buffer(buffer_b.clone(), cx)
    });
    buffer_a.update(cx_a, |buf, cx| buf.edit([(0..0, "hi-a, ")], None, cx));
    save_b.await.unwrap();
    assert_eq!(
        client_a.fs().load("/a/file1.rs".as_ref()).await.unwrap(),
        "hi-a, i-am-c, i-am-b, i-am-a"
    );

    executor.run_until_parked();

    buffer_a.read_with(cx_a, |buf, _| assert!(!buf.is_dirty()));

    buffer_b.read_with(cx_b, |buf, _| assert!(!buf.is_dirty()));

    buffer_c.read_with(cx_c, |buf, _| assert!(!buf.is_dirty()));

    // Make changes on host's file system, see those changes on guest worktrees.
    client_a
        .fs()
        .rename(
            path!("/a/file1.rs").as_ref(),
            path!("/a/file1.js").as_ref(),
            Default::default(),
        )
        .await
        .unwrap();
    client_a
        .fs()
        .rename(
            path!("/a/file2").as_ref(),
            path!("/a/file3").as_ref(),
            Default::default(),
        )
        .await
        .unwrap();
    client_a
        .fs()
        .insert_file(path!("/a/file4"), "4".into())
        .await;
    executor.run_until_parked();

    worktree_a.read_with(cx_a, |tree, _| {
        assert_eq!(
            tree.paths().collect::<Vec<_>>(),
            [rel_path("file1.js"), rel_path("file3"), rel_path("file4")]
        )
    });

    worktree_b.read_with(cx_b, |tree, _| {
        assert_eq!(
            tree.paths().collect::<Vec<_>>(),
            [rel_path("file1.js"), rel_path("file3"), rel_path("file4")]
        )
    });

    worktree_c.read_with(cx_c, |tree, _| {
        assert_eq!(
            tree.paths().collect::<Vec<_>>(),
            [rel_path("file1.js"), rel_path("file3"), rel_path("file4")]
        )
    });

    // Ensure buffer files are updated as well.

    buffer_a.read_with(cx_a, |buffer, _| {
        assert_eq!(buffer.file().unwrap().path().as_ref(), rel_path("file1.js"));
        assert_eq!(buffer.language().unwrap().name(), "JavaScript");
    });

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(buffer.file().unwrap().path().as_ref(), rel_path("file1.js"));
        assert_eq!(buffer.language().unwrap().name(), "JavaScript");
    });

    buffer_c.read_with(cx_c, |buffer, _| {
        assert_eq!(buffer.file().unwrap().path().as_ref(), rel_path("file1.js"));
        assert_eq!(buffer.language().unwrap().name(), "JavaScript");
    });

    let new_buffer_a = project_a
        .update(cx_a, |p, cx| p.create_buffer(None, false, cx))
        .await
        .unwrap();

    let new_buffer_id = new_buffer_a.read_with(cx_a, |buffer, _| buffer.remote_id());
    let new_buffer_b = project_b
        .update(cx_b, |p, cx| p.open_buffer_by_id(new_buffer_id, cx))
        .await
        .unwrap();

    new_buffer_b.read_with(cx_b, |buffer, _| {
        assert!(buffer.file().is_none());
    });

    new_buffer_a.update(cx_a, |buffer, cx| {
        buffer.edit([(0..0, "ok")], None, cx);
    });
    project_a
        .update(cx_a, |project, cx| {
            let path = ProjectPath {
                path: rel_path("file3.rs").into(),
                worktree_id: worktree_a.read(cx).id(),
            };

            project.save_buffer_as(new_buffer_a.clone(), path, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    new_buffer_b.read_with(cx_b, |buffer_b, _| {
        assert_eq!(
            buffer_b.file().unwrap().path().as_ref(),
            rel_path("file3.rs")
        );

        new_buffer_a.read_with(cx_a, |buffer_a, _| {
            assert_eq!(buffer_b.saved_mtime(), buffer_a.saved_mtime());
            assert_eq!(buffer_b.saved_version(), buffer_a.saved_version());
        });
    });
}
