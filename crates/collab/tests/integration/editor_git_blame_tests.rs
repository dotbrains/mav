use crate::TestServer;
use call::ActiveCall;
use editor::{Editor, RowInfo};
use git::repository::repo_path;
use gpui::{TestAppContext, UpdateGlobal};
use pretty_assertions::assert_eq;
use serde_json::json;
use settings::{InlineBlameSettings, SettingsStore};
use std::{ops::Range, path::Path};
use text::Point;
use util::{path, rel_path::rel_path};

#[gpui::test(iterations = 10)]
async fn test_git_blame_is_forwarded(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);
    // Turn inline-blame-off by default so no state is transferred without us explicitly doing so
    let inline_blame_off_settings = Some(InlineBlameSettings {
        enabled: Some(false),
        ..Default::default()
    });
    cx_a.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git.get_or_insert_default().inline_blame = inline_blame_off_settings;
            });
        });
    });
    cx_b.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.git.get_or_insert_default().inline_blame = inline_blame_off_settings;
            });
        });
    });

    client_a
        .fs()
        .insert_tree(
            path!("/my-repo"),
            json!({
                ".git": {},
                "file.txt": "line1\nline2\nline3\nline\n",
            }),
        )
        .await;

    let blame = git::blame::Blame {
        entries: vec![
            blame_entry("1b1b1b", 0..1),
            blame_entry("0d0d0d", 1..2),
            blame_entry("3a3a3a", 2..3),
            blame_entry("4c4c4c", 3..4),
        ],
        messages: [
            ("1b1b1b", "message for idx-0"),
            ("0d0d0d", "message for idx-1"),
            ("3a3a3a", "message for idx-2"),
            ("4c4c4c", "message for idx-3"),
        ]
        .into_iter()
        .map(|(sha, message)| (sha.parse().unwrap(), message.into()))
        .collect(),
    };
    client_a.fs().set_blame_for_repo(
        Path::new(path!("/my-repo/.git")),
        vec![(repo_path("file.txt"), blame)],
    );

    let (project_a, worktree_id) = client_a.build_local_project(path!("/my-repo"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // Create editor_a
    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("file.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Join the project as client B.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("file.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let buffer_id_b = editor_b.update(cx_b, |editor_b, cx| {
        editor_b
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .read(cx)
            .remote_id()
    });

    // client_b now requests git blame for the open buffer
    editor_b.update_in(cx_b, |editor_b, window, cx| {
        assert!(editor_b.blame().is_none());
        editor_b.toggle_git_blame(&git::Blame {}, window, cx);
    });

    cx_a.executor().run_until_parked();
    cx_b.executor().run_until_parked();

    editor_b.update(cx_b, |editor_b, cx| {
        let blame = editor_b.blame().expect("editor_b should have blame now");
        let entries = blame.update(cx, |blame, cx| {
            blame
                .blame_for_rows(
                    &(0..4)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
                            buffer_id: Some(buffer_id_b),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx,
                )
                .collect::<Vec<_>>()
        });

        assert_eq!(
            entries,
            vec![
                Some((buffer_id_b, blame_entry("1b1b1b", 0..1))),
                Some((buffer_id_b, blame_entry("0d0d0d", 1..2))),
                Some((buffer_id_b, blame_entry("3a3a3a", 2..3))),
                Some((buffer_id_b, blame_entry("4c4c4c", 3..4))),
            ]
        );

        blame.update(cx, |blame, _| {
            for (idx, (buffer, entry)) in entries.iter().flatten().enumerate() {
                let details = blame.details_for_entry(*buffer, entry).unwrap();
                assert_eq!(details.message, format!("message for idx-{}", idx));
            }
        });
    });

    // editor_b updates the file, which gets sent to client_a, which updates git blame,
    // which gets back to client_b.
    editor_b.update_in(cx_b, |editor_b, _, cx| {
        editor_b.edit([(Point::new(0, 3)..Point::new(0, 3), "FOO")], cx);
    });

    cx_a.executor().run_until_parked();
    cx_b.executor().run_until_parked();

    editor_b.update(cx_b, |editor_b, cx| {
        let blame = editor_b.blame().expect("editor_b should have blame now");
        let entries = blame.update(cx, |blame, cx| {
            blame
                .blame_for_rows(
                    &(0..4)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
                            buffer_id: Some(buffer_id_b),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx,
                )
                .collect::<Vec<_>>()
        });

        assert_eq!(
            entries,
            vec![
                None,
                Some((buffer_id_b, blame_entry("0d0d0d", 1..2))),
                Some((buffer_id_b, blame_entry("3a3a3a", 2..3))),
                Some((buffer_id_b, blame_entry("4c4c4c", 3..4))),
            ]
        );
    });

    // Now editor_a also updates the file
    editor_a.update_in(cx_a, |editor_a, _, cx| {
        editor_a.edit([(Point::new(1, 3)..Point::new(1, 3), "FOO")], cx);
    });

    cx_a.executor().run_until_parked();
    cx_b.executor().run_until_parked();

    editor_b.update(cx_b, |editor_b, cx| {
        let blame = editor_b.blame().expect("editor_b should have blame now");
        let entries = blame.update(cx, |blame, cx| {
            blame
                .blame_for_rows(
                    &(0..4)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
                            buffer_id: Some(buffer_id_b),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx,
                )
                .collect::<Vec<_>>()
        });

        assert_eq!(
            entries,
            vec![
                None,
                None,
                Some((buffer_id_b, blame_entry("3a3a3a", 2..3))),
                Some((buffer_id_b, blame_entry("4c4c4c", 3..4))),
            ]
        );
    });
}

fn blame_entry(sha: &str, range: Range<u32>) -> git::blame::BlameEntry {
    git::blame::BlameEntry {
        sha: sha.parse().unwrap(),
        range,
        original_line_number: 0,
        author: None,
        author_mail: None,
        author_time: None,
        author_tz: None,
        committer_name: None,
        committer_email: None,
        committer_time: None,
        committer_tz: None,
        summary: None,
        previous: None,
        filename: String::new(),
    }
}
