use crate::{
    RoomParticipants, TestClient, TestServer, following_tests::join_channel, room_participants,
};
use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use collab::rpc::RECONNECT_TIMEOUT;
use collections::{HashMap, HashSet};
use futures::StreamExt as _;
use gpui::{
    App, BackgroundExecutor, Modifiers, MouseButton, MouseDownEvent, TestAppContext, px, size,
};
use language::{FakeLspAdapter, OffsetRangeExt, rust_lang};
use lsp::OneOf;
use pretty_assertions::assert_eq;
use project::{
    HoverBlockKind, ProjectPath,
    lsp_store::SymbolLocation,
    search::{SearchQuery, SearchResult},
};
use rand::prelude::*;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use util::{path, rel_path::rel_path, uri};
use workspace::Pane;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test(iterations = 10)]
async fn test_project_search(
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
            "/root",
            json!({
                "dir-1": {
                    "a": "hello world",
                    "b": "goodnight moon",
                    "c": "a world of goo",
                    "d": "world champion of clown world",
                },
                "dir-2": {
                    "e": "disney world is fun",
                }
            }),
        )
        .await;
    let (project_a, _) = client_a.build_local_project("/root/dir-1", cx_a).await;
    let (worktree_2, _) = project_a
        .update(cx_a, |p, cx| {
            p.find_or_create_worktree("/root/dir-2", true, cx)
        })
        .await
        .unwrap();
    worktree_2
        .read_with(cx_a, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Perform a search as the guest.
    let mut results = HashMap::default();
    let search_rx = project_b.update(cx_b, |project, cx| {
        project.search(
            SearchQuery::text(
                "world",
                false,
                false,
                false,
                Default::default(),
                Default::default(),
                false,
                None,
            )
            .unwrap(),
            cx,
        )
    });
    while let Ok(result) = search_rx.rx.recv().await {
        match result {
            SearchResult::Buffer { buffer, ranges } => {
                results.entry(buffer).or_insert(ranges);
            }
            SearchResult::LimitReached => {
                panic!(
                    "Unexpectedly reached search limit in tests. If you do want to assert limit-reached, change this panic call."
                )
            }
            SearchResult::WaitingForScan | SearchResult::Searching => {}
        };
    }

    let mut ranges_by_path = results
        .into_iter()
        .map(|(buffer, ranges)| {
            buffer.read_with(cx_b, |buffer, cx| {
                let path = buffer.file().unwrap().full_path(cx);
                let offset_ranges = ranges
                    .into_iter()
                    .map(|range| range.to_offset(buffer))
                    .collect::<Vec<_>>();
                (path, offset_ranges)
            })
        })
        .collect::<Vec<_>>();
    ranges_by_path.sort_by_key(|(path, _)| path.clone());

    assert_eq!(
        ranges_by_path,
        &[
            (PathBuf::from("dir-1/a"), vec![6..11]),
            (PathBuf::from("dir-1/c"), vec![2..7]),
            (PathBuf::from("dir-1/d"), vec![0..5, 24..29]),
            (PathBuf::from("dir-2/e"), vec![7..12]),
        ]
    );
}

#[gpui::test(iterations = 10)]
async fn test_document_highlights(
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
            path!("/root-1"),
            json!({
                "main.rs": "fn double(number: i32) -> i32 { number + number }",
            }),
        )
        .await;

    client_a.language_registry().add(rust_lang());
    let capabilities = lsp::ServerCapabilities {
        document_highlight_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
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

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root-1"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open the file on client B.
    let (buffer_b, _handle) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    // Request document highlights as the guest.
    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::request::DocumentHighlightRequest, _, _>(
        |params, _| async move {
            assert_eq!(
                params
                    .text_document_position_params
                    .text_document
                    .uri
                    .as_str(),
                uri!("file:///root-1/main.rs")
            );
            assert_eq!(
                params.text_document_position_params.position,
                lsp::Position::new(0, 34)
            );
            Ok(Some(vec![
                lsp::DocumentHighlight {
                    kind: Some(lsp::DocumentHighlightKind::WRITE),
                    range: lsp::Range::new(lsp::Position::new(0, 10), lsp::Position::new(0, 16)),
                },
                lsp::DocumentHighlight {
                    kind: Some(lsp::DocumentHighlightKind::READ),
                    range: lsp::Range::new(lsp::Position::new(0, 32), lsp::Position::new(0, 38)),
                },
                lsp::DocumentHighlight {
                    kind: Some(lsp::DocumentHighlightKind::READ),
                    range: lsp::Range::new(lsp::Position::new(0, 41), lsp::Position::new(0, 47)),
                },
            ]))
        },
    );
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    let highlights = project_b
        .update(cx_b, |p, cx| p.document_highlights(&buffer_b, 34, cx))
        .await
        .unwrap();

    buffer_b.read_with(cx_b, |buffer, _| {
        let snapshot = buffer.snapshot();

        let highlights = highlights
            .into_iter()
            .map(|highlight| (highlight.kind, highlight.range.to_offset(&snapshot)))
            .collect::<Vec<_>>();
        assert_eq!(
            highlights,
            &[
                (lsp::DocumentHighlightKind::WRITE, 10..16),
                (lsp::DocumentHighlightKind::READ, 32..38),
                (lsp::DocumentHighlightKind::READ, 41..47)
            ]
        )
    });
}

#[gpui::test(iterations = 10)]
async fn test_lsp_hover(
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
            path!("/root-1"),
            json!({
                "main.rs": "use std::collections::HashMap;",
            }),
        )
        .await;

    client_a.language_registry().add(rust_lang());
    let language_server_names = ["rust-analyzer", "CrabLang-ls"];
    let capabilities_1 = lsp::ServerCapabilities {
        hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
        ..lsp::ServerCapabilities::default()
    };
    let capabilities_2 = lsp::ServerCapabilities {
        hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
        ..lsp::ServerCapabilities::default()
    };
    let mut language_servers = [
        client_a.language_registry().register_fake_lsp(
            "Rust",
            FakeLspAdapter {
                name: language_server_names[0],
                capabilities: capabilities_1.clone(),
                ..FakeLspAdapter::default()
            },
        ),
        client_a.language_registry().register_fake_lsp(
            "Rust",
            FakeLspAdapter {
                name: language_server_names[1],
                capabilities: capabilities_2.clone(),
                ..FakeLspAdapter::default()
            },
        ),
    ];
    client_b.language_registry().add(rust_lang());
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: language_server_names[0],
            capabilities: capabilities_1,
            ..FakeLspAdapter::default()
        },
    );
    client_b.language_registry().register_fake_lsp_adapter(
        "Rust",
        FakeLspAdapter {
            name: language_server_names[1],
            capabilities: capabilities_2,
            ..FakeLspAdapter::default()
        },
    );

    let (project_a, worktree_id) = client_a.build_local_project(path!("/root-1"), cx_a).await;
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Open the file as the guest
    let (buffer_b, _handle) = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let mut servers_with_hover_requests = HashMap::default();
    for i in 0..language_server_names.len() {
        let new_server = language_servers[i].next().await.unwrap_or_else(|| {
            panic!(
                "Failed to get language server #{i} with name {}",
                &language_server_names[i]
            )
        });
        let new_server_name = new_server.server.name();
        assert!(
            !servers_with_hover_requests.contains_key(&new_server_name),
            "Unexpected: initialized server with the same name twice. Name: `{new_server_name}`"
        );
        match new_server_name.as_ref() {
            "CrabLang-ls" => {
                servers_with_hover_requests.insert(
                    new_server_name.clone(),
                    new_server.set_request_handler::<lsp::request::HoverRequest, _, _>(
                        move |params, _| {
                            assert_eq!(
                                params
                                    .text_document_position_params
                                    .text_document
                                    .uri
                                    .as_str(),
                                uri!("file:///root-1/main.rs")
                            );
                            let name = new_server_name.clone();
                            async move {
                                Ok(Some(lsp::Hover {
                                    contents: lsp::HoverContents::Scalar(
                                        lsp::MarkedString::String(format!("{name} hover")),
                                    ),
                                    range: None,
                                }))
                            }
                        },
                    ),
                );
            }
            "rust-analyzer" => {
                servers_with_hover_requests.insert(
                    new_server_name.clone(),
                    new_server.set_request_handler::<lsp::request::HoverRequest, _, _>(
                        |params, _| async move {
                            assert_eq!(
                                params
                                    .text_document_position_params
                                    .text_document
                                    .uri
                                    .as_str(),
                                uri!("file:///root-1/main.rs")
                            );
                            assert_eq!(
                                params.text_document_position_params.position,
                                lsp::Position::new(0, 22)
                            );
                            Ok(Some(lsp::Hover {
                                contents: lsp::HoverContents::Array(vec![
                                    lsp::MarkedString::String("Test hover content.".to_string()),
                                    lsp::MarkedString::LanguageString(lsp::LanguageString {
                                        language: "Rust".to_string(),
                                        value: "let foo = 42;".to_string(),
                                    }),
                                ]),
                                range: Some(lsp::Range::new(
                                    lsp::Position::new(0, 22),
                                    lsp::Position::new(0, 29),
                                )),
                            }))
                        },
                    ),
                );
            }
            unexpected => panic!("Unexpected server name: {unexpected}"),
        }
    }
    cx_a.run_until_parked();
    cx_b.run_until_parked();

    // Request hover information as the guest.
    let mut hovers = project_b
        .update(cx_b, |p, cx| p.hover(&buffer_b, 22, cx))
        .await
        .unwrap();
    assert_eq!(
        hovers.len(),
        2,
        "Expected two hovers from both language servers, but got: {hovers:?}"
    );

    let _: Vec<()> = futures::future::join_all(servers_with_hover_requests.into_values().map(
        |mut hover_request| async move {
            hover_request
                .next()
                .await
                .expect("All hover requests should have been triggered")
        },
    ))
    .await;

    hovers.sort_by_key(|hover| hover.contents.len());
    let first_hover = hovers.first().cloned().unwrap();
    assert_eq!(
        first_hover.contents,
        vec![project::HoverBlock {
            text: "CrabLang-ls hover".to_string(),
            kind: HoverBlockKind::Markdown,
        },]
    );
    let second_hover = hovers.last().cloned().unwrap();
    assert_eq!(
        second_hover.contents,
        vec![
            project::HoverBlock {
                text: "Test hover content.".to_string(),
                kind: HoverBlockKind::Markdown,
            },
            project::HoverBlock {
                text: "let foo = 42;".to_string(),
                kind: HoverBlockKind::Code {
                    language: "Rust".to_string()
                },
            }
        ]
    );
    buffer_b.read_with(cx_b, |buffer, _| {
        let snapshot = buffer.snapshot();
        assert_eq!(second_hover.range.unwrap().to_offset(&snapshot), 22..29);
    });
}

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

#[gpui::test(iterations = 10)]
async fn test_contacts(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_d: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_d = server.create_client(cx_d, "user_d").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);
    let _active_call_d = cx_d.read(ActiveCall::global);

    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    server.disconnect_client(client_c.peer_id().unwrap());
    server.forbid_connections();
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "offline", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "offline", "free")
        ]
    );
    assert_eq!(contacts(&client_c, cx_c), []);
    assert_eq!(contacts(&client_d, cx_d), []);

    server.allow_connections();
    client_c
        .connect(false, &cx_c.to_async())
        .await
        .into_response()
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    // Client B and client D become contacts while client B is being called.
    server
        .make_contacts(&mut [(&client_b, cx_b), (&client_d, cx_d)])
        .await;
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free"),
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    active_call_b.update(cx_b, |call, cx| call.decline_incoming(cx).unwrap());
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_c
        .update(cx_c, |call, cx| {
            call.invite(client_a.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert_eq!(contacts(&client_a, cx_a), []);
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    // Test removing a contact
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.remove_contact(client_c.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [("user_a".to_string(), "offline", "free"),]
    );

    fn contacts(
        client: &TestClient,
        cx: &TestAppContext,
    ) -> Vec<(String, &'static str, &'static str)> {
        client.user_store().read_with(cx, |store, _| {
            store
                .contacts()
                .iter()
                .map(|contact| {
                    (
                        contact.user.username.clone().to_string(),
                        if contact.online { "online" } else { "offline" },
                        if contact.busy { "busy" } else { "free" },
                    )
                })
                .collect()
        })
    }
}

#[gpui::test(iterations = 10)]
async fn test_contact_requests(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_a2: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_b2: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_c2: &mut TestAppContext,
) {
    // Connect to a server as 3 clients.
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_a2 = server.create_client(cx_a2, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_b2 = server.create_client(cx_b2, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_c2 = server.create_client(cx_c2, "user_c").await;

    assert_eq!(client_a.user_id().unwrap(), client_a2.user_id().unwrap());
    assert_eq!(client_b.user_id().unwrap(), client_b2.user_id().unwrap());
    assert_eq!(client_c.user_id().unwrap(), client_c2.user_id().unwrap());

    // User A and User C request that user B become their contact.
    client_a
        .user_store()
        .update(cx_a, |store, cx| {
            store.request_contact(client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    client_c
        .user_store()
        .update(cx_c, |store, cx| {
            store.request_contact(client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // All users see the pending request appear in all their clients.
    assert_eq!(
        client_a.summarize_contacts(cx_a).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_a2.summarize_contacts(cx_a2).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_b2.summarize_contacts(cx_b2).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_c2.summarize_contacts(cx_c2).outgoing_requests,
        &["user_b"]
    );

    // Contact requests are present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(
        client_a.summarize_contacts(cx_a).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );

    // User B accepts the request from user A.
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.respond_to_contact_request(client_a.user_id().unwrap(), true, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // User B sees user A as their contact now in all client, and the incoming request from them is removed.
    let contacts_b = client_b.summarize_contacts(cx_b);
    assert_eq!(contacts_b.current, &["user_a"]);
    assert_eq!(contacts_b.incoming_requests, &["user_c"]);
    let contacts_b2 = client_b2.summarize_contacts(cx_b2);
    assert_eq!(contacts_b2.current, &["user_a"]);
    assert_eq!(contacts_b2.incoming_requests, &["user_c"]);

    // User A sees user B as their contact now in all clients, and the outgoing request to them is removed.
    let contacts_a = client_a.summarize_contacts(cx_a);
    assert_eq!(contacts_a.current, &["user_b"]);
    assert!(contacts_a.outgoing_requests.is_empty());
    let contacts_a2 = client_a2.summarize_contacts(cx_a2);
    assert_eq!(contacts_a2.current, &["user_b"]);
    assert!(contacts_a2.outgoing_requests.is_empty());

    // Contacts are present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(client_a.summarize_contacts(cx_a).current, &["user_b"]);
    assert_eq!(client_b.summarize_contacts(cx_b).current, &["user_a"]);
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_c"]
    );
    assert!(client_c.summarize_contacts(cx_c).current.is_empty());
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );

    // User B rejects the request from user C.
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.respond_to_contact_request(client_c.user_id().unwrap(), false, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // User B doesn't see user C as their contact, and the incoming request from them is removed.
    let contacts_b = client_b.summarize_contacts(cx_b);
    assert_eq!(contacts_b.current, &["user_a"]);
    assert!(contacts_b.incoming_requests.is_empty());
    let contacts_b2 = client_b2.summarize_contacts(cx_b2);
    assert_eq!(contacts_b2.current, &["user_a"]);
    assert!(contacts_b2.incoming_requests.is_empty());

    // User C doesn't see user B as their contact, and the outgoing request to them is removed.
    let contacts_c = client_c.summarize_contacts(cx_c);
    assert!(contacts_c.current.is_empty());
    assert!(contacts_c.outgoing_requests.is_empty());
    let contacts_c2 = client_c2.summarize_contacts(cx_c2);
    assert!(contacts_c2.current.is_empty());
    assert!(contacts_c2.outgoing_requests.is_empty());

    // Incoming/outgoing requests are not present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(client_a.summarize_contacts(cx_a).current, &["user_b"]);
    assert_eq!(client_b.summarize_contacts(cx_b).current, &["user_a"]);
    assert!(
        client_b
            .summarize_contacts(cx_b)
            .incoming_requests
            .is_empty()
    );
    assert!(client_c.summarize_contacts(cx_c).current.is_empty());
    assert!(
        client_c
            .summarize_contacts(cx_c)
            .outgoing_requests
            .is_empty()
    );

    async fn disconnect_and_reconnect(client: &TestClient, cx: &mut TestAppContext) {
        client.disconnect(&cx.to_async());
        client.clear_contacts(cx).await;
        client
            .connect(false, &cx.to_async())
            .await
            .into_response()
            .unwrap();
    }
}

#[gpui::test(iterations = 10)]
async fn test_join_call_after_screen_was_shared(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // Call users B and C from client A.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec!["user_b".to_string()]
        }
    );

    // User B receives the call.

    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    let call_b = incoming_call_b.next().await.unwrap().unwrap();
    assert_eq!(call_b.calling_user.username, "user_a");

    // User A shares their screen
    let display = gpui::TestScreenCaptureSource::new();
    cx_a.set_screen_capture_sources(vec![display]);
    let screen_a = cx_a
        .update(|cx| cx.screen_capture_sources())
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen(screen_a, cx))
        })
        .await
        .unwrap();

    client_b.user_store().update(cx_b, |user_store, _| {
        user_store.clear_cache();
    });

    // User B joins the room
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    assert!(incoming_call_b.next().await.unwrap().is_none());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec![],
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec![],
        }
    );

    // Ensure User B sees User A's screenshare.

    room_b.read_with(cx_b, |room, _| {
        assert_eq!(
            room.remote_participants()
                .get(&client_a.user_id().unwrap())
                .unwrap()
                .video_tracks
                .len(),
            1
        );
    });
}

#[cfg(target_os = "linux")]
#[gpui::test(iterations = 10)]
async fn test_share_screen_wayland(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A calls user B.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts.
    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    executor.run_until_parked();
    incoming_call_b.next().await.unwrap().unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    // User A shares their screen via the Wayland path.
    let events_b = active_call_events(cx_b);
    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen_wayland(cx))
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Room A is sharing and has a nonzero synthetic screen ID.
    room_a.read_with(cx_a, |room, _| {
        assert!(room.is_sharing_screen());
        let screen_id = room.shared_screen_id();
        assert!(screen_id.is_some(), "shared_screen_id should be Some");
        assert_ne!(screen_id.unwrap(), 0, "synthetic ID must be nonzero");
    });

    // User B observes the remote screen sharing track.
    assert_eq!(events_b.borrow().len(), 1);
    if let call::room::Event::RemoteVideoTracksChanged { participant_id } =
        events_b.borrow().first().unwrap()
    {
        assert_eq!(*participant_id, client_a.peer_id().unwrap());
        room_b.read_with(cx_b, |room, _| {
            assert_eq!(
                room.remote_participants()[&client_a.user_id().unwrap()]
                    .video_tracks
                    .len(),
                1
            );
        });
    } else {
        panic!("expected RemoteVideoTracksChanged event");
    }
}

#[cfg(target_os = "linux")]
#[gpui::test(iterations = 10)]
async fn test_unshare_screen_wayland(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    // User A calls user B.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User B accepts.
    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    executor.run_until_parked();
    incoming_call_b.next().await.unwrap().unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    // User A shares their screen via the Wayland path.
    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen_wayland(cx))
        })
        .await
        .unwrap();
    executor.run_until_parked();

    room_a.read_with(cx_a, |room, _| {
        assert!(room.is_sharing_screen());
    });

    // User A stops sharing.
    room_a
        .update(cx_a, |room, cx| room.unshare_screen(true, cx))
        .unwrap();
    executor.run_until_parked();

    // Room A is no longer sharing, screen ID is gone.
    room_a.read_with(cx_a, |room, _| {
        assert!(!room.is_sharing_screen());
        assert!(room.shared_screen_id().is_none());
    });
}

#[gpui::test]
async fn test_right_click_menu_behind_collab_panel(cx: &mut TestAppContext) {
    let mut server = TestServer::start(cx.executor().clone()).await;
    let client_a = server.create_client(cx, "user_a").await;
    let (_workspace_a, cx) = client_a.build_test_workspace(cx).await;

    cx.simulate_resize(size(px(300.), px(300.)));

    cx.simulate_keystrokes("cmd-n cmd-n cmd-n");
    cx.update(|window, _cx| window.refresh());

    let new_tab_button_bounds = cx.debug_bounds("ICON-Plus").unwrap();

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: new_tab_button_bounds.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });

    // regression test that the right click menu for tabs does not open.
    assert!(cx.debug_bounds("MENU_ITEM-Close").is_none());

    let tab_bounds = cx.debug_bounds("TAB-1").unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: tab_bounds.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    assert!(cx.debug_bounds("MENU_ITEM-Close").is_some());
}

#[gpui::test]
async fn test_pane_split_left(cx: &mut TestAppContext) {
    let (_, client) = TestServer::start1(cx).await;
    let (workspace, cx) = client.build_test_workspace(cx).await;

    cx.simulate_keystrokes("cmd-n");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 1);
    });
    cx.simulate_keystrokes("cmd-k left");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 2);
    });
    cx.simulate_keystrokes("cmd-k");
    // Sleep past the historical timeout to ensure the multi-stroke binding
    // still fires now that unambiguous prefixes no longer auto-expire.
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.simulate_keystrokes("left");
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.items(cx).collect::<Vec<_>>().len() == 3);
    });
}

#[gpui::test]
async fn test_join_after_restart(cx1: &mut TestAppContext, cx2: &mut TestAppContext) {
    let (mut server, client) = TestServer::start1(cx1).await;
    let channel1 = server.make_public_channel("channel1", &client, cx1).await;
    let channel2 = server.make_public_channel("channel2", &client, cx1).await;

    join_channel(channel1, &client, cx1).await.unwrap();
    drop(client);

    let client2 = server.create_client(cx2, "user_a").await;
    join_channel(channel2, &client2, cx2).await.unwrap();
}

#[gpui::test]
async fn test_preview_tabs(cx: &mut TestAppContext) {
    let (_server, client) = TestServer::start1(cx).await;
    let (workspace, cx) = client.build_test_workspace(cx).await;
    let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());

    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let path_1 = ProjectPath {
        worktree_id,
        path: rel_path("1.txt").into(),
    };
    let path_2 = ProjectPath {
        worktree_id,
        path: rel_path("2.js").into(),
    };
    let path_3 = ProjectPath {
        worktree_id,
        path: rel_path("3.rs").into(),
    };

    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let get_path = |pane: &Pane, idx: usize, cx: &App| {
        pane.item_for_index(idx).unwrap().project_path(cx).unwrap()
    };

    // Opening item 3 as a "permanent" tab
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(path_3.clone(), None, false, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(!pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_1.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 2 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Going back should show item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Closing item 1
    pane.update_in(cx, |pane, window, cx| {
        pane.close_item_by_id(
            pane.active_item().unwrap().item_id(),
            workspace::SaveIntent::Skip,
            window,
            cx,
        )
    })
    .await
    .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Going back should show item 1 as preview
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.go_back(pane.downgrade(), window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_3.clone());
        assert_eq!(get_path(pane, 1, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Close permanent tab
    pane.update_in(cx, |pane, window, cx| {
        let id = pane.items().next().unwrap().item_id();
        pane.close_item_by_id(id, workspace::SaveIntent::Skip, window, cx)
    })
    .await
    .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    // Split pane to the right
    pane.update_in(cx, |pane, window, cx| {
        pane.split(
            workspace::SplitDirection::Right,
            workspace::SplitMode::default(),
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let right_pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    right_pane.update(cx, |pane, cx| {
        // Nav history is now cloned in an pane split, but that's inconvenient
        // for this test, which uses the presence of a backwards history item as
        // an indication that a preview item was successfully opened
        pane.nav_history_mut().clear(cx);
    });

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(pane.preview_item_id(), None);

        assert!(!pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Open item 2 as preview in right pane
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    // Focus left pane
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.activate_pane_in_direction(workspace::SplitDirection::Left, window, cx)
    });

    // Open item 2 as preview in left pane
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path_preview(path_2.clone(), None, true, true, true, window, cx)
        })
        .await
        .unwrap();

    pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 1);
        assert_eq!(get_path(pane, 0, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().next().unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });

    right_pane.update(cx, |pane, cx| {
        assert_eq!(pane.items_len(), 2);
        assert_eq!(get_path(pane, 0, cx), path_1.clone());
        assert_eq!(get_path(pane, 1, cx), path_2.clone());
        assert_eq!(
            pane.preview_item_id(),
            Some(pane.items().nth(1).unwrap().item_id())
        );

        assert!(pane.can_navigate_backward());
        assert!(!pane.can_navigate_forward());
    });
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
