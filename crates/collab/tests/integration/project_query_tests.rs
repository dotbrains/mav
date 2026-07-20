use call::ActiveCall;
use collections::HashMap;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};
use language::{FakeLspAdapter, OffsetRangeExt, rust_lang};
use pretty_assertions::assert_eq;
use project::{
    HoverBlockKind,
    search::{SearchQuery, SearchResult},
};
use serde_json::json;
use std::path::PathBuf;
use util::{path, rel_path::rel_path, uri};

use crate::TestServer;

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
