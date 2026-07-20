use crate::TestServer;
use call::ActiveCall;
use futures::StreamExt;
use gpui::{SharedString, TestAppContext};
use language::{FakeLspAdapter, rust_lang};
use pretty_assertions::assert_eq;
use project::{ProgressToken, SERVER_PROGRESS_THROTTLE_TIMEOUT};
use serde_json::json;
use util::path;

#[gpui::test(iterations = 10)]
async fn test_language_server_statuses(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let executor = cx_a.executor();
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_b.update(editor::init);

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                "main.rs": "const ONE: usize = 1;",
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project(path!("/dir"), cx_a).await;

    let _buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/main.rs"), cx)
        })
        .await
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();
    fake_language_server.start_progress("the-token").await;

    executor.advance_clock(SERVER_PROGRESS_THROTTLE_TIMEOUT);
    fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::NumberOrString::String("the-token".to_string()),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::Report(
            lsp::WorkDoneProgressReport {
                message: Some("the-message".to_string()),
                ..Default::default()
            },
        )),
    });
    executor.run_until_parked();

    let token = ProgressToken::String(SharedString::from("the-token"));

    project_a.read_with(cx_a, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "the-language-server");
        assert_eq!(status.pending_work.len(), 1);
        assert_eq!(
            status.pending_work[&token].message.as_ref().unwrap(),
            "the-message"
        );
    });

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    project_b.read_with(cx_b, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "the-language-server");
    });

    executor.advance_clock(SERVER_PROGRESS_THROTTLE_TIMEOUT);
    fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::NumberOrString::String("the-token".to_string()),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::Report(
            lsp::WorkDoneProgressReport {
                message: Some("the-message-2".to_string()),
                ..Default::default()
            },
        )),
    });
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "the-language-server");
        assert_eq!(status.pending_work.len(), 1);
        assert_eq!(
            status.pending_work[&token].message.as_ref().unwrap(),
            "the-message-2"
        );
    });

    project_b.read_with(cx_b, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "the-language-server");
        assert_eq!(status.pending_work.len(), 1);
        assert_eq!(
            status.pending_work[&token].message.as_ref().unwrap(),
            "the-message-2"
        );
    });
}

#[gpui::test]
async fn test_local_registration_for_new_available_server_from_remote(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let executor = cx_a.executor();
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    client_b.language_registry().add(rust_lang());

    // Client B has an "available" adapter for "the-language-server",
    // but it's not regitstered for Rust
    client_b
        .language_registry()
        .register_fake_available_lsp_adapter(
            "the-language-server",
            FakeLspAdapter {
                name: "the-language-server",
                ..Default::default()
            },
        );

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                "main.rs": "const ONE: usize = 1;",
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project(path!("/dir"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    executor.run_until_parked();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Client A starts the language server.
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );

    let _buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/main.rs"), cx)
        })
        .await
        .unwrap();

    let _fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    // Verify client B has registered the adapter for Rust locally
    project_b.read_with(cx_b, |project, cx| {
        let statuses = project.language_server_statuses(cx).collect::<Vec<_>>();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].1.name.0, "the-language-server");
    });

    let rust_adapters = client_b
        .language_registry()
        .lsp_adapters(&language::LanguageName::new("Rust"));
    assert!(
        rust_adapters
            .iter()
            .any(|a| a.name().0 == "the-language-server")
    );
}

#[gpui::test]
async fn test_local_registration_for_existing_available_server_from_remote(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(cx_a.executor()).await;
    let executor = cx_a.executor();
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a.language_registry().add(rust_lang());
    client_b.language_registry().add(rust_lang());

    // Client B has an "available" adapter for "the-language-server",
    // but it's not regitstered for Rust
    client_b
        .language_registry()
        .register_fake_available_lsp_adapter(
            "the-language-server",
            FakeLspAdapter {
                name: "the-language-server",
                ..Default::default()
            },
        );

    client_a
        .fs()
        .insert_tree(
            path!("/dir"),
            json!({
                "main.rs": "const ONE: usize = 1;",
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project(path!("/dir"), cx_a).await;

    // Client A starts the language server FIRST.
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );

    let _buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/main.rs"), cx)
        })
        .await
        .unwrap();

    let _fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    executor.run_until_parked();

    // Client B joins the remote project.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    // Verify client B has registered the adapter for Rust locally.
    let rust_adapters = client_b
        .language_registry()
        .lsp_adapters(&language::LanguageName::new("Rust"));
    assert!(
        rust_adapters
            .iter()
            .any(|a| a.name().0 == "the-language-server"),
        "Adapter should have been registered upon joining"
    );

    project_b.read_with(cx_b, |project, cx| {
        let statuses = project.language_server_statuses(cx).collect::<Vec<_>>();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].1.name.0, "the-language-server");
    });
}
