use call::ActiveCall;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};
use language::{
    Diagnostic, DiagnosticEntry, DiagnosticSourceKind, FakeLspAdapter, Language, LanguageConfig,
    LanguageMatcher, Point, rust_lang, tree_sitter_rust,
};
use lsp::{DEFAULT_LSP_REQUEST_TIMEOUT, LanguageServerId};
use pretty_assertions::assert_eq;
use project::{DiagnosticSummary, ProjectPath};
use serde_json::json;
use std::{
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};
use util::{path, rel_path::rel_path};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_collaborating_with_diagnostics(
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

    client_a.language_registry().add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    let mut fake_language_servers = client_a
        .language_registry()
        .register_fake_lsp("Rust", Default::default());

    // Share a project as client A
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "a.rs": "let one = two",
                "other.rs": "",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;

    // Cause the language server to start.
    let _buffer = project_a
        .update(cx_a, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/other.rs"), cx)
        })
        .await
        .unwrap();

    // Simulate a language server reporting errors for a file.
    let mut fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server
        .receive_notification::<lsp::notification::DidOpenTextDocument>()
        .await;
    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/a.rs")).unwrap(),
            version: None,
            diagnostics: vec![lsp::Diagnostic {
                severity: Some(lsp::DiagnosticSeverity::WARNING),
                range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 7)),
                message: "message 0".to_string(),
                ..Default::default()
            }],
        },
    );

    // Client A shares the project and, simultaneously, the language server
    // publishes a diagnostic. This is done to ensure that the server always
    // observes the latest diagnostics for a worktree.
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/a.rs")).unwrap(),
            version: None,
            diagnostics: vec![lsp::Diagnostic {
                severity: Some(lsp::DiagnosticSeverity::ERROR),
                range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 7)),
                message: "message 1".to_string(),
                ..Default::default()
            }],
        },
    );

    // Join the worktree as client B.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;

    // Wait for server to see the diagnostics update.
    executor.run_until_parked();

    // Ensure client B observes the new diagnostics.

    project_b.read_with(cx_b, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            &[(
                ProjectPath {
                    worktree_id,
                    path: rel_path("a.rs").into(),
                },
                LanguageServerId(0),
                DiagnosticSummary {
                    error_count: 1,
                    warning_count: 0,
                },
            )]
        )
    });

    // Join project as client C and observe the diagnostics.
    let project_c = client_c.join_remote_project(project_id, cx_c).await;
    executor.run_until_parked();
    let project_c_diagnostic_summaries =
        Rc::new(RefCell::new(project_c.read_with(cx_c, |project, cx| {
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>()
        })));
    project_c.update(cx_c, |_, cx| {
        let summaries = project_c_diagnostic_summaries.clone();
        cx.subscribe(&project_c, {
            move |p, _, event, cx| {
                if let project::Event::DiskBasedDiagnosticsFinished { .. } = event {
                    *summaries.borrow_mut() = p.diagnostic_summaries(false, cx).collect();
                }
            }
        })
        .detach();
    });

    executor.run_until_parked();
    assert_eq!(
        project_c_diagnostic_summaries.borrow().as_slice(),
        &[(
            ProjectPath {
                worktree_id,
                path: rel_path("a.rs").into(),
            },
            LanguageServerId(0),
            DiagnosticSummary {
                error_count: 1,
                warning_count: 0,
            },
        )]
    );

    // Simulate a language server reporting more errors for a file.
    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/a.rs")).unwrap(),
            version: None,
            diagnostics: vec![
                lsp::Diagnostic {
                    severity: Some(lsp::DiagnosticSeverity::ERROR),
                    range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 7)),
                    message: "message 1".to_string(),
                    ..Default::default()
                },
                lsp::Diagnostic {
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    range: lsp::Range::new(lsp::Position::new(0, 10), lsp::Position::new(0, 13)),
                    message: "message 2".to_string(),
                    ..Default::default()
                },
            ],
        },
    );

    // Clients B and C get the updated summaries
    executor.run_until_parked();

    project_b.read_with(cx_b, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            [(
                ProjectPath {
                    worktree_id,
                    path: rel_path("a.rs").into(),
                },
                LanguageServerId(0),
                DiagnosticSummary {
                    error_count: 1,
                    warning_count: 1,
                },
            )]
        );
    });

    project_c.read_with(cx_c, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            [(
                ProjectPath {
                    worktree_id,
                    path: rel_path("a.rs").into(),
                },
                LanguageServerId(0),
                DiagnosticSummary {
                    error_count: 1,
                    warning_count: 1,
                },
            )]
        );
    });

    // Open the file with the errors on client B. They should be present.
    let open_buffer = project_b.update(cx_b, |p, cx| {
        p.open_buffer((worktree_id, rel_path("a.rs")), cx)
    });
    let buffer_b = cx_b.executor().spawn(open_buffer).await.unwrap();

    buffer_b.read_with(cx_b, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, Point>(0..buffer.len(), false)
                .collect::<Vec<_>>(),
            &[
                DiagnosticEntry {
                    range: Point::new(0, 4)..Point::new(0, 7),
                    diagnostic: Diagnostic {
                        group_id: 2,
                        message: "message 1".to_string(),
                        severity: lsp::DiagnosticSeverity::ERROR,
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    }
                },
                DiagnosticEntry {
                    range: Point::new(0, 10)..Point::new(0, 13),
                    diagnostic: Diagnostic {
                        group_id: 3,
                        severity: lsp::DiagnosticSeverity::WARNING,
                        message: "message 2".to_string(),
                        is_primary: true,
                        source_kind: DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    }
                }
            ]
        );
    });

    // Simulate a language server reporting no errors for a file.
    fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
        lsp::PublishDiagnosticsParams {
            uri: lsp::Uri::from_file_path(path!("/a/a.rs")).unwrap(),
            version: None,
            diagnostics: Vec::new(),
        },
    );
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            []
        )
    });

    project_b.read_with(cx_b, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            []
        )
    });

    project_c.read_with(cx_c, |project, cx| {
        assert_eq!(
            project.diagnostic_summaries(false, cx).collect::<Vec<_>>(),
            []
        )
    });
}

#[gpui::test(iterations = 10)]
async fn test_collaborating_with_lsp_progress_updates_and_diagnostics_ordering(
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

    client_a.language_registry().add(rust_lang());
    let mut fake_language_servers = client_a.language_registry().register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            disk_based_diagnostics_progress_token: Some("the-disk-based-token".into()),
            disk_based_diagnostics_sources: vec!["the-disk-based-diagnostics-source".into()],
            ..Default::default()
        },
    );

    let file_names = &["one.rs", "two.rs", "three.rs", "four.rs", "five.rs"];
    client_a
        .fs()
        .insert_tree(
            path!("/test"),
            json!({
                "one.rs": "const ONE: usize = 1;",
                "two.rs": "const TWO: usize = 2;",
                "three.rs": "const THREE: usize = 3;",
                "four.rs": "const FOUR: usize = 3;",
                "five.rs": "const FIVE: usize = 3;",
            }),
        )
        .await;

    let (project_a, worktree_id) = client_a.build_local_project(path!("/test"), cx_a).await;

    // Share a project as client A
    let active_call_a = cx_a.read(ActiveCall::global);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    // Join the project as client B and open all three files.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let guest_buffers = futures::future::try_join_all(file_names.iter().map(|file_name| {
        project_b.update(cx_b, |p, cx| {
            p.open_buffer_with_lsp((worktree_id, rel_path(file_name)), cx)
        })
    }))
    .await
    .unwrap();

    // Simulate a language server reporting errors for a file.
    let fake_language_server = fake_language_servers.next().await.unwrap();
    executor.run_until_parked();
    fake_language_server
        .request::<lsp::request::WorkDoneProgressCreate>(
            lsp::WorkDoneProgressCreateParams {
                token: lsp::NumberOrString::String("the-disk-based-token".to_string()),
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::NumberOrString::String("the-disk-based-token".to_string()),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::Begin(
            lsp::WorkDoneProgressBegin {
                title: "Progress Began".into(),
                ..Default::default()
            },
        )),
    });
    for file_name in file_names {
        fake_language_server.notify::<lsp::notification::PublishDiagnostics>(
            lsp::PublishDiagnosticsParams {
                uri: lsp::Uri::from_file_path(Path::new(path!("/test")).join(file_name)).unwrap(),
                version: None,
                diagnostics: vec![lsp::Diagnostic {
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    source: Some("the-disk-based-diagnostics-source".into()),
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                    message: "message one".to_string(),
                    ..Default::default()
                }],
            },
        );
    }
    fake_language_server.notify::<lsp::notification::Progress>(lsp::ProgressParams {
        token: lsp::NumberOrString::String("the-disk-based-token".to_string()),
        value: lsp::ProgressParamsValue::WorkDone(lsp::WorkDoneProgress::End(
            lsp::WorkDoneProgressEnd { message: None },
        )),
    });

    // When the "disk base diagnostics finished" message is received, the buffers'
    // diagnostics are expected to be present.
    let disk_based_diagnostics_finished = Arc::new(AtomicBool::new(false));
    project_b.update(cx_b, {
        let project_b = project_b.clone();
        let disk_based_diagnostics_finished = disk_based_diagnostics_finished.clone();
        move |_, cx| {
            cx.subscribe(&project_b, move |_, _, event, cx| {
                if let project::Event::DiskBasedDiagnosticsFinished { .. } = event {
                    disk_based_diagnostics_finished.store(true, SeqCst);
                    for (buffer, _) in &guest_buffers {
                        assert_eq!(
                            buffer
                                .read(cx)
                                .snapshot()
                                .diagnostics_in_range::<_, usize>(0..5, false)
                                .count(),
                            1,
                            "expected a diagnostic for buffer {:?}",
                            buffer.read(cx).file().unwrap().path(),
                        );
                    }
                }
            })
            .detach();
        }
    });

    executor.run_until_parked();
    assert!(disk_based_diagnostics_finished.load(SeqCst));
}
