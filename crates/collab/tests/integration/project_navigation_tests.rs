use anyhow::{Result, anyhow};
use call::ActiveCall;
use futures::{StreamExt as _, channel::mpsc};
use gpui::{BackgroundExecutor, TestAppContext};
use language::{FakeLspAdapter, OffsetRangeExt, Point, rust_lang};
use lsp::OneOf;
use parking_lot::Mutex;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::{path::Path, sync::Arc};
use util::{path, rel_path::rel_path, uri};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_definition(
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

    let capabilities = lsp::ServerCapabilities {
        definition_provider: Some(OneOf::Left(true)),
        type_definition_provider: Some(lsp::TypeDefinitionProviderCapability::Simple(true)),
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
                "dir-1": {
                    "a.rs": "const ONE: usize = b::TWO + b::THREE;",
                },
                "dir-2": {
                    "b.rs": "const TWO: c::T2 = 2;\nconst THREE: usize = 3;",
                    "c.rs": "type T2 = usize;",
                }
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/root/dir-1"), cx_a)
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open the file on client B.
    let (buffer_b, _handle) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();

    // Request the definition of a symbol as the guest.
    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::request::GotoDefinition, _, _>(
        |_, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Scalar(
                lsp::Location::new(
                    lsp::Uri::from_file_path(path!("/root/dir-2/b.rs")).unwrap(),
                    lsp::Range::new(lsp::Position::new(0, 6), lsp::Position::new(0, 9)),
                ),
            )))
        },
    );
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let definitions_1 = project_b
        .update(cx_b, |p, cx| p.definitions(&buffer_b, 23, cx))
        .await
        .unwrap()
        .unwrap();
    cx_b.read(|cx| {
        assert_eq!(
            definitions_1.len(),
            1,
            "Unexpected definitions: {definitions_1:?}"
        );
        assert_eq!(project_b.read(cx).worktrees(cx).count(), 2);
        let target_buffer = definitions_1[0].target.buffer.read(cx);
        assert_eq!(
            target_buffer.text(),
            "const TWO: c::T2 = 2;\nconst THREE: usize = 3;"
        );
        assert_eq!(
            definitions_1[0].target.range.to_point(target_buffer),
            Point::new(0, 6)..Point::new(0, 9)
        );
    });

    // Try getting more definitions for the same buffer, ensuring the buffer gets reused from
    // the previous call to `definition`.
    fake_language_server.set_request_handler::<lsp::request::GotoDefinition, _, _>(
        |_, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Scalar(
                lsp::Location::new(
                    lsp::Uri::from_file_path(path!("/root/dir-2/b.rs")).unwrap(),
                    lsp::Range::new(lsp::Position::new(1, 6), lsp::Position::new(1, 11)),
                ),
            )))
        },
    );

    let definitions_2 = project_b
        .update(cx_b, |p, cx| p.definitions(&buffer_b, 33, cx))
        .await
        .unwrap()
        .unwrap();
    cx_b.read(|cx| {
        assert_eq!(definitions_2.len(), 1);
        assert_eq!(project_b.read(cx).worktrees(cx).count(), 2);
        let target_buffer = definitions_2[0].target.buffer.read(cx);
        assert_eq!(
            target_buffer.text(),
            "const TWO: c::T2 = 2;\nconst THREE: usize = 3;"
        );
        assert_eq!(
            definitions_2[0].target.range.to_point(target_buffer),
            Point::new(1, 6)..Point::new(1, 11)
        );
    });
    assert_eq!(
        definitions_1[0].target.buffer,
        definitions_2[0].target.buffer
    );

    fake_language_server.set_request_handler::<lsp::request::GotoTypeDefinition, _, _>(
        |req, _| async move {
            assert_eq!(
                req.text_document_position_params.position,
                lsp::Position::new(0, 7)
            );
            Ok(Some(lsp::GotoDefinitionResponse::Scalar(
                lsp::Location::new(
                    lsp::Uri::from_file_path(path!("/root/dir-2/c.rs")).unwrap(),
                    lsp::Range::new(lsp::Position::new(0, 5), lsp::Position::new(0, 7)),
                ),
            )))
        },
    );

    let type_definitions = project_b
        .update(cx_b, |p, cx| p.type_definitions(&buffer_b, 7, cx))
        .await
        .unwrap()
        .unwrap();
    cx_b.read(|cx| {
        assert_eq!(
            type_definitions.len(),
            1,
            "Unexpected type definitions: {type_definitions:?}"
        );
        let target_buffer = type_definitions[0].target.buffer.read(cx);
        assert_eq!(target_buffer.text(), "type T2 = usize;");
        assert_eq!(
            type_definitions[0].target.range.to_point(target_buffer),
            Point::new(0, 5)..Point::new(0, 7)
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_references(
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

    let capabilities = lsp::ServerCapabilities {
        references_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "my-fake-lsp-adapter",
            capabilities: capabilities.clone(),
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: "my-fake-lsp-adapter",
            capabilities,
            ..FakeLspAdapter::default()
        },
    );

    client_a
        .fs()
        .insert_tree(
            path!("/root"),
            json!({
                "dir-1": {
                    "one.rs": "const ONE: usize = 1;",
                    "two.rs": "const TWO: usize = one::ONE + one::ONE;",
                },
                "dir-2": {
                    "three.rs": "const THREE: usize = two::TWO + one::ONE;",
                }
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a
        .build_local_project(path!("/root/dir-1"), cx_a)
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open the file on client B.
    let (buffer_b, _handle) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("one.rs")), cx)
        })
        .await
        .unwrap();

    // Request references to a symbol as the guest.
    let fake_language_server = fake_language_servers.next().await.unwrap();
    let (lsp_response_tx, rx) = mpsc::unbounded::<Result<Option<Vec<lsp::Location>>>>();
    fake_language_server.set_request_handler::<lsp::request::References, _, _>({
        let rx = Arc::new(Mutex::new(Some(rx)));
        move |params, _| {
            assert_eq!(
                params.text_document_position.text_document.uri.as_str(),
                uri!("file:///root/dir-1/one.rs")
            );
            let rx = rx.clone();
            async move {
                let mut response_rx = rx.lock().take().unwrap();
                let result = response_rx.next().await.unwrap();
                *rx.lock() = Some(response_rx);
                result
            }
        }
    });
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let references = project_b.update(cx_b, |p, cx| p.references(&buffer_b, 7, cx));

    // User is informed that a request is pending.
    executor.run_until_parked();
    project_b.read_with(cx_b, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "my-fake-lsp-adapter");
        assert_eq!(
            status.pending_work.values().next().unwrap().message,
            Some("Finding references...".into())
        );
    });

    // Cause the language server to respond.
    lsp_response_tx
        .unbounded_send(Ok(Some(vec![
            lsp::Location {
                uri: lsp::Uri::from_file_path(path!("/root/dir-1/two.rs")).unwrap(),
                range: lsp::Range::new(lsp::Position::new(0, 24), lsp::Position::new(0, 27)),
            },
            lsp::Location {
                uri: lsp::Uri::from_file_path(path!("/root/dir-1/two.rs")).unwrap(),
                range: lsp::Range::new(lsp::Position::new(0, 35), lsp::Position::new(0, 38)),
            },
            lsp::Location {
                uri: lsp::Uri::from_file_path(path!("/root/dir-2/three.rs")).unwrap(),
                range: lsp::Range::new(lsp::Position::new(0, 37), lsp::Position::new(0, 40)),
            },
        ])))
        .unwrap();

    let references = references.await.unwrap().unwrap();
    executor.run_until_parked();
    project_b.read_with(cx_b, |project, cx| {
        // User is informed that a request is no longer pending.
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert!(status.pending_work.is_empty());

        assert_eq!(references.len(), 3);
        assert_eq!(project.worktrees(cx).count(), 2);

        let two_buffer = references[0].buffer.read(cx);
        let three_buffer = references[2].buffer.read(cx);
        assert_eq!(
            two_buffer.file().unwrap().path().as_ref(),
            rel_path("two.rs")
        );
        assert_eq!(references[1].buffer, references[0].buffer);
        assert_eq!(
            three_buffer.file().unwrap().full_path(cx),
            Path::new(path!("/root/dir-2/three.rs"))
        );

        assert_eq!(references[0].range.to_offset(two_buffer), 24..27);
        assert_eq!(references[1].range.to_offset(two_buffer), 35..38);
        assert_eq!(references[2].range.to_offset(three_buffer), 37..40);
    });

    let references = project_b.update(cx_b, |p, cx| p.references(&buffer_b, 7, cx));

    // User is informed that a request is pending.
    executor.run_until_parked();
    project_b.read_with(cx_b, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert_eq!(status.name.0, "my-fake-lsp-adapter");
        assert_eq!(
            status.pending_work.values().next().unwrap().message,
            Some("Finding references...".into())
        );
    });

    // Cause the LSP request to fail.
    lsp_response_tx
        .unbounded_send(Err(anyhow!("can't find references")))
        .unwrap();
    assert_eq!(references.await.unwrap().unwrap(), []);

    // User is informed that the request is no longer pending.
    executor.run_until_parked();
    project_b.read_with(cx_b, |project, cx| {
        let status = project.language_server_statuses(cx).next().unwrap().1;
        assert!(status.pending_work.is_empty());
    });
}
