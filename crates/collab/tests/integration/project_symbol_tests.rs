use call::ActiveCall;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};
use language::{FakeLspAdapter, rust_lang};
use lsp::OneOf;
use pretty_assertions::assert_eq;
use project::lsp_store::SymbolLocation;
use rand::prelude::*;
use serde_json::json;
use std::path::Path;
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_project_symbols(
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

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/code"),
            json!({
                "crate-1": {
                    "one.rs": "const ONE: usize = 1;",
                },
                "crate-2": {
                    "two.rs": "const TWO: usize = 2; const THREE: usize = 3;",
                },
                "private": {
                    "passwords.txt": "the-password",
                }
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/code/crate-1"), cx_a)
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Cause the language server to start.
    let _buffer = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("one.rs")), cx)
        })
        .await
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();
    fake_language_server.set_request_handler::<lsp::WorkspaceSymbolRequest, _, _>(
        |_, _| async move {
            Ok(Some(lsp::WorkspaceSymbolResponse::Flat(vec![
                #[allow(deprecated)]
                lsp::SymbolInformation {
                    name: "TWO".into(),
                    location: lsp::Location {
                        uri: lsp::Uri::from_file_path(path!("/code/crate-2/two.rs")).unwrap(),
                        range: lsp::Range::new(lsp::Position::new(0, 6), lsp::Position::new(0, 9)),
                    },
                    kind: lsp::SymbolKind::CONSTANT,
                    tags: None,
                    container_name: None,
                    deprecated: None,
                },
            ])))
        },
    );

    // Request the definition of a symbol as the guest.
    let symbols = project_b
        .update(cx_b, |p, cx| p.symbols("two", cx))
        .await
        .unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "TWO");

    // Open one of the returned symbols.
    let buffer_b_2 = project_b
        .update(cx_b, |project, cx| {
            project.open_buffer_for_symbol(&symbols[0], cx)
        })
        .await
        .unwrap();

    buffer_b_2.read_with(cx_b, |buffer, cx| {
        assert_eq!(
            buffer.file().unwrap().full_path(cx),
            Path::new(path!("/code/crate-2/two.rs"))
        );
    });

    // Attempt to craft a symbol and violate host's privacy by opening an arbitrary file.
    let mut fake_symbol = symbols[0].clone();
    fake_symbol.path = SymbolLocation::OutsideProject {
        abs_path: Path::new(path!("/code/secrets")).into(),
        signature: [0x17; 32],
    };
    let error = project_b
        .update(cx_b, |project, cx| {
            project.open_buffer_for_symbol(&fake_symbol, cx)
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("invalid symbol signature"));
}

#[gpui::test(iterations = 10)]
async fn test_open_buffer_while_getting_definition_pointing_to_it(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    mut rng: StdRng,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    let capabilities = lsp::ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: capabilities.clone(),
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            capabilities,
            ..FakeLspAdapter::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "a.rs": "const ONE: usize = b::TWO;",
                "b.rs": "const TWO: usize = 2",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/root"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    let (buffer_b1, _lsp) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::request::GotoDefinition, _, _>(
        |_, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Scalar(
                lsp::Location::new(
                    lsp::Uri::from_file_path(path!("/root/b.rs")).unwrap(),
                    lsp::Range::new(lsp::Position::new(0, 6), lsp::Position::new(0, 9)),
                ),
            )))
        },
    );

    let definitions;
    let buffer_b2;
    if rng.random() {
        cx_a.run_until_parked();
        cx_b.run_until_parked();
        definitions = project_b.update(cx_b, |p, cx| p.definitions(&buffer_b1, 23, cx));
        (buffer_b2, _) = project_b
            .update(cx_b, |p, cx| {
                p.open_buffer_with_lsp((worktree_id, rel_path("b.rs")), cx)
            })
            .await
            .unwrap();
    } else {
        (buffer_b2, _) = project_b
            .update(cx_b, |p, cx| {
                p.open_buffer_with_lsp((worktree_id, rel_path("b.rs")), cx)
            })
            .await
            .unwrap();
        cx_a.run_until_parked();
        cx_b.run_until_parked();
        definitions = project_b.update(cx_b, |p, cx| p.definitions(&buffer_b1, 23, cx));
    }

    let definitions = definitions.await.unwrap().unwrap();
    assert_eq!(
        definitions.len(),
        1,
        "Unexpected definitions: {definitions:?}"
    );
    assert_eq!(definitions[0].target.buffer, buffer_b2);
}
