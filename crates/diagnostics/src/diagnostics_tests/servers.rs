use super::*;

#[gpui::test]
async fn test_diagnostics_multiple_servers(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            "main.js": "
                a();
                b();
                c();
                d();
                e();
            ".unindent()
        }),
    )
    .await;

    let server_id_1 = LanguageServerId(100);
    let server_id_2 = LanguageServerId(101);
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });
    let editor = diagnostics.update(cx, |diagnostics, _| diagnostics.editor.clone());

    // Two language servers start updating diagnostics
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_started(server_id_1, cx);
        lsp_store.disk_based_diagnostics_started(server_id_2, cx);
        lsp_store
            .update_diagnostics(
                server_id_1,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 1)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "error 1".to_string(),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
    });

    // The first language server finishes
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_finished(server_id_1, cx);
    });

    // Only the first language server's diagnostics are shown.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.executor().run_until_parked();

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             a(); § error 1
             b();
             c();"
        }
    );

    // The second language server finishes
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                server_id_2,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(1, 0), lsp::Position::new(1, 1)),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "warning 1".to_string(),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store.disk_based_diagnostics_finished(server_id_2, cx);
    });

    // Both language server's diagnostics are shown.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.executor().run_until_parked();

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             a(); § error 1
             b(); § warning 1
             c();
             d();"
        }
    );

    // Both language servers start updating diagnostics, and the first server finishes.
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store.disk_based_diagnostics_started(server_id_1, cx);
        lsp_store.disk_based_diagnostics_started(server_id_2, cx);
        lsp_store
            .update_diagnostics(
                server_id_1,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(2, 0), lsp::Position::new(2, 1)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "warning 2".to_string(),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store
            .update_diagnostics(
                server_id_2,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.rs")).unwrap(),
                    diagnostics: vec![],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store.disk_based_diagnostics_finished(server_id_1, cx);
    });

    // Only the first language server's diagnostics are updated.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.executor().run_until_parked();

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             a();
             b(); § warning 1
             c(); § warning 2
             d();
             e();"
        }
    );

    // The second language server finishes.
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                server_id_2,
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/test/main.js")).unwrap(),
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(3, 0), lsp::Position::new(3, 1)),
                        severity: Some(lsp::DiagnosticSeverity::WARNING),
                        message: "warning 2".to_string(),
                        ..Default::default()
                    }],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
        lsp_store.disk_based_diagnostics_finished(server_id_2, cx);
    });

    // Both language servers' diagnostics are updated.
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.executor().run_until_parked();

    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
                 § -----
                 a();
                 b();
                 c(); § warning 2
                 d(); § warning 2
                 e();"
        }
    );
}

#[gpui::test(iterations = 20)]
async fn test_random_diagnostics_blocks(cx: &mut TestAppContext, mut rng: StdRng) {
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

    log::info!("constructing reference diagnostics view");
    let reference_diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });
    cx.executor()
        .advance_clock(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10));
    cx.run_until_parked();

    let mutated_excerpts =
        editor_content_with_blocks(&mutated_diagnostics.update(cx, |d, _| d.editor.clone()), cx);
    let reference_excerpts = editor_content_with_blocks(
        &reference_diagnostics.update(cx, |d, _| d.editor.clone()),
        cx,
    );

    // The mutated view may contain more than the reference view as
    // we don't currently shrink excerpts when diagnostics were removed.
    let mut ref_iter = reference_excerpts.lines().filter(|line| {
        // ignore $ ---- and $ <file>.rs
        !line.starts_with('§')
            || line.starts_with("§ diagnostic")
            || line.starts_with("§ related info")
    });
    let mut next_ref_line = ref_iter.next();
    let mut skipped_block = false;

    for mut_line in mutated_excerpts.lines() {
        if let Some(ref_line) = next_ref_line {
            if mut_line == ref_line {
                next_ref_line = ref_iter.next();
            } else if mut_line.contains('§')
                // ignore $ ---- and $ <file>.rs
                && (!mut_line.starts_with('§')
                    || mut_line.starts_with("§ diagnostic")
                    || mut_line.starts_with("§ related info"))
            {
                skipped_block = true;
            }
        }
    }

    if next_ref_line.is_some() || skipped_block {
        pretty_assertions::assert_eq!(mutated_excerpts, reference_excerpts);
    }
}

// similar to above, but with inlays. Used to find panics when mixing diagnostics and inlays.
