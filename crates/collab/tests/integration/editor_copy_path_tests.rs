use crate::TestServer;
use call::ActiveCall;
use editor::{
    Editor, MultiBufferOffset,
    actions::{CopyFileLocation, CopyFileName, CopyFileNameWithoutExtension},
};
use gpui::TestAppContext;
use indoc::indoc;
use pretty_assertions::assert_eq;
use serde_json::json;
use util::{path, rel_path::rel_path};

#[gpui::test]
async fn test_copy_file_name_without_extension(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.copy_file_name_without_extension(&CopyFileNameWithoutExtension, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some("main".to_string())
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.copy_file_name_without_extension(&CopyFileNameWithoutExtension, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some("main".to_string())
    );
}

#[gpui::test]
async fn test_copy_file_name(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.copy_file_name(&CopyFileName, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some("main.rs".to_string())
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.copy_file_name(&CopyFileName, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some("main.rs".to_string())
    );
}

#[gpui::test]
async fn test_copy_file_location(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "src": {
                    "main.rs": indoc! {"
                        fn main() {
                            println!(\"Hello, world!\");
                        }
                    "},
                }
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("src/main.rs")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    cx_a.run_until_parked();
    cx_b.run_until_parked();

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(16)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(16)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(44)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2-3", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(44)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2-3", path!("src/main.rs")))
    );

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(43)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_a.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );

    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(16)..MultiBufferOffset(43)]);
        });
        editor.copy_file_location(&CopyFileLocation, window, cx);
    });

    assert_eq!(
        cx_b.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("{}:2", path!("src/main.rs")))
    );
}
