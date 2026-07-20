use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_single_file_worktrees_diagnostics(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.rs": "let a = 1;",
            "b.rs": "let b = 2;"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [path!("/dir/a.rs").as_ref(), path!("/dir/b.rs").as_ref()],
        cx,
    )
    .await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/dir/b.rs"), cx)
        })
        .await
        .unwrap();

    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                LanguageServerId(0),
                lsp::PublishDiagnosticsParams {
                    uri: Uri::from_file_path(path!("/dir/a.rs")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 5)),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "error 1".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store
            .update_diagnostics(
                LanguageServerId(0),
                lsp::PublishDiagnosticsParams {
                    uri: Uri::from_file_path(path!("/dir/b.rs")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 5)),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: "error 2".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
    });

    buffer_a.update(cx, |buffer, _| {
        let chunks = chunks_with_diagnostics(buffer, 0..buffer.len());
        assert_eq!(
            chunks
                .iter()
                .map(|(s, d)| (s.as_str(), *d))
                .collect::<Vec<_>>(),
            &[
                ("let ", None),
                ("a", Some(DiagnosticSeverity::ERROR)),
                (" = 1;", None),
            ]
        );
    });
    buffer_b.update(cx, |buffer, _| {
        let chunks = chunks_with_diagnostics(buffer, 0..buffer.len());
        assert_eq!(
            chunks
                .iter()
                .map(|(s, d)| (s.as_str(), *d))
                .collect::<Vec<_>>(),
            &[
                ("let ", None),
                ("b", Some(DiagnosticSeverity::WARNING)),
                (" = 2;", None),
            ]
        );
    });
}

#[gpui::test]
async fn test_omitted_diagnostics(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir": {
                ".git": {
                    "HEAD": "ref: refs/heads/main",
                },
                ".gitignore": "b.rs",
                "a.rs": "let a = 1;",
                "b.rs": "let b = 2;",
            },
            "other.rs": "let b = c;"
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root/dir").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/root/dir"), true, cx)
        })
        .await
        .unwrap();
    let main_worktree_id = worktree.read_with(cx, |tree, _| tree.id());

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/root/other.rs"), false, cx)
        })
        .await
        .unwrap();
    let other_worktree_id = worktree.update(cx, |tree, _| tree.id());

    let server_id = LanguageServerId(0);
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                server_id,
                lsp::PublishDiagnosticsParams {
                    uri: Uri::from_file_path(path!("/root/dir/b.rs")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 4), lsp::Position::new(0, 5)),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "unused variable 'b'".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store
            .update_diagnostics(
                server_id,
                lsp::PublishDiagnosticsParams {
                    uri: Uri::from_file_path(path!("/root/other.rs")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 8), lsp::Position::new(0, 9)),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "unknown variable 'c'".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
    });

    let main_ignored_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((main_worktree_id, rel_path("b.rs")), cx)
        })
        .await
        .unwrap();
    main_ignored_buffer.update(cx, |buffer, _| {
        let chunks = chunks_with_diagnostics(buffer, 0..buffer.len());
        assert_eq!(
            chunks
                .iter()
                .map(|(s, d)| (s.as_str(), *d))
                .collect::<Vec<_>>(),
            &[
                ("let ", None),
                ("b", Some(DiagnosticSeverity::ERROR)),
                (" = 2;", None),
            ],
            "Gigitnored buffers should still get in-buffer diagnostics",
        );
    });
    let other_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((other_worktree_id, rel_path("")), cx)
        })
        .await
        .unwrap();
    other_buffer.update(cx, |buffer, _| {
        let chunks = chunks_with_diagnostics(buffer, 0..buffer.len());
        assert_eq!(
            chunks
                .iter()
                .map(|(s, d)| (s.as_str(), *d))
                .collect::<Vec<_>>(),
            &[
                ("let b = ", None),
                ("c", Some(DiagnosticSeverity::ERROR)),
                (";", None),
            ],
            "Buffers from hidden projects should still get in-buffer diagnostics"
        );
    });

    project.update(cx, |project, cx| {
        assert_eq!(project.diagnostic_summaries(false, cx).next(), None);
        assert_eq!(
            project.diagnostic_summaries(true, cx).collect::<Vec<_>>(),
            vec![(
                ProjectPath {
                    worktree_id: main_worktree_id,
                    path: rel_path("b.rs").into(),
                },
                server_id,
                DiagnosticSummary {
                    error_count: 1,
                    warning_count: 0,
                }
            )]
        );
        assert_eq!(project.diagnostic_summary(false, cx).error_count, 0);
        assert_eq!(project.diagnostic_summary(true, cx).error_count, 1);
    });
}
