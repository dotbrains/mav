use super::*;

#[gpui::test]
async fn test_random_diagnostics_with_inlays(cx: &mut TestAppContext, mut rng: StdRng) {
    init_test(cx);

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let mutated_diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_center(Box::new(mutated_diagnostics.clone()), window, cx);
    });
    mutated_diagnostics.update_in(cx, |diagnostics, window, _cx| {
        assert!(diagnostics.focus_handle.is_focused(window));
    });

    let mut next_id = 0;
    let mut next_filename = 0;
    let mut language_server_ids = vec![LanguageServerId(0)];
    let mut updated_language_servers = HashSet::default();
    let mut current_diagnostics: HashMap<(PathBuf, LanguageServerId), Vec<lsp::Diagnostic>> =
        Default::default();
    let mut next_inlay_id = 0;

    for _ in 0..operations {
        match rng.random_range(0..100) {
            // language server completes its diagnostic check
            0..=20 if !updated_language_servers.is_empty() => {
                let server_id = *updated_language_servers.iter().choose(&mut rng).unwrap();
                log::info!("finishing diagnostic check for language server {server_id}");
                lsp_store.update(cx, |lsp_store, cx| {
                    lsp_store.disk_based_diagnostics_finished(server_id, cx)
                });

                if rng.random_bool(0.5) {
                    cx.run_until_parked();
                }
            }

            21..=50 => mutated_diagnostics.update_in(cx, |diagnostics, window, cx| {
                diagnostics.editor.update(cx, |editor, cx| {
                    let snapshot = editor.snapshot(window, cx);
                    if !snapshot.buffer_snapshot().is_empty() {
                        let position = rng
                            .random_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len());
                        let position = snapshot.buffer_snapshot().clip_offset(position, Bias::Left);
                        log::info!(
                            "adding inlay at {position}/{}: {:?}",
                            snapshot.buffer_snapshot().len(),
                            snapshot.buffer_snapshot().text(),
                        );

                        editor.splice_inlays(
                            &[],
                            vec![Inlay::edit_prediction(
                                post_inc(&mut next_inlay_id),
                                snapshot.buffer_snapshot().anchor_before(position),
                                Rope::from_iter(["Test inlay ", "next_inlay_id"]),
                            )],
                            cx,
                        );
                    }
                });
            }),

            // language server updates diagnostics
            _ => {
                let (path, server_id, diagnostics) =
                    match current_diagnostics.iter_mut().choose(&mut rng) {
                        // update existing set of diagnostics
                        Some(((path, server_id), diagnostics)) if rng.random_bool(0.5) => {
                            (path.clone(), *server_id, diagnostics)
                        }

                        // insert a set of diagnostics for a new path
                        _ => {
                            let path: PathBuf =
                                format!(path!("/test/{}.rs"), post_inc(&mut next_filename)).into();
                            let len = rng.random_range(128..256);
                            let content =
                                RandomCharIter::new(&mut rng).take(len).collect::<String>();
                            fs.insert_file(&path, content.into_bytes()).await;

                            let server_id = match language_server_ids.iter().choose(&mut rng) {
                                Some(server_id) if rng.random_bool(0.5) => *server_id,
                                _ => {
                                    let id = LanguageServerId(language_server_ids.len());
                                    language_server_ids.push(id);
                                    id
                                }
                            };

                            (
                                path.clone(),
                                server_id,
                                current_diagnostics.entry((path, server_id)).or_default(),
                            )
                        }
                    };

                updated_language_servers.insert(server_id);

                lsp_store.update(cx, |lsp_store, cx| {
                    log::info!("updating diagnostics. language server {server_id} path {path:?}");
                    randomly_update_diagnostics_for_path(
                        &fs,
                        &path,
                        diagnostics,
                        &mut next_id,
                        &mut rng,
                    );
                    lsp_store
                        .update_diagnostics(
                            server_id,
                            lsp::PublishDiagnosticsParams {
                                uri: lsp::Uri::from_file_path(&path).unwrap_or_else(|_| {
                                    lsp::Uri::from_str("file:///test/fallback.rs").unwrap()
                                }),
                                diagnostics: diagnostics.clone(),
                                version: None,
                            },
                            None,
                            DiagnosticSourceKind::Pushed,
                            &[],
                            cx,
                        )
                        .unwrap()
                });
                cx.executor()
                    .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));

                cx.run_until_parked();
            }
        }
    }

    log::info!("updating mutated diagnostics view");
    mutated_diagnostics.update_in(cx, |diagnostics, window, cx| {
        diagnostics.update_stale_excerpts(window, cx)
    });

    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.run_until_parked();
}

#[gpui::test]
async fn active_diagnostics_dismiss_after_invalidation(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.set_state(indoc! {"
        ˇfn func(abc def: i32) -> u32 {
        }
    "});

    let message = "Something's wrong!";
    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![lsp::Diagnostic {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 11),
                                lsp::Position::new(0, 12),
                            ),
                            severity: Some(lsp::DiagnosticSeverity::ERROR),
                            message: message.to_string(),
                            ..Default::default()
                        }],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
        assert_eq!(
            editor.active_diagnostic_message(),
            Some(message),
            "Should have a diagnostics group activated"
        );
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: Vec::new(),
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.active_diagnostic_message(), None);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
        assert_eq!(editor.active_diagnostic_message(), None);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});
}

#[gpui::test]
async fn cycle_through_same_place_diagnostics(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.set_state(indoc! {"
        ˇfn func(abc def: i32) -> u32 {
        }
    "});

    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 11),
                                    lsp::Position::new(0, 12),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 25),
                                    lsp::Position::new(0, 28),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                        ],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });
    cx.run_until_parked();

    //// Backward

    // Fourth diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    // Third diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    // Second diagnostic, same place
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    // First diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    // Wrapped over, fourth diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        ˇfn func(abc def: i32) -> u32 {
        }
    "});

    //// Forward

    // First diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    // Second diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    // Third diagnostic, same place
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    // Fourth diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    // Wrapped around, first diagnostic
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});
}
