use super::*;

#[gpui::test]
async fn go_to_diagnostic_with_severity(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.set_state(indoc! {"error warning info hiˇnt"});

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
                                    lsp::Position::new(0, 0),
                                    lsp::Position::new(0, 5),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 6),
                                    lsp::Position::new(0, 13),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::WARNING),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 14),
                                    lsp::Position::new(0, 18),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::INFORMATION),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 19),
                                    lsp::Position::new(0, 23),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::HINT),
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

    macro_rules! go {
        ($severity:expr) => {
            cx.update_editor(|editor, window, cx| {
                editor.go_to_diagnostic(
                    &GoToDiagnostic {
                        severity: $severity,
                    },
                    window,
                    cx,
                );
            });
        };
    }

    // Default, should cycle through all diagnostics
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"error warning info ˇhint"});
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"ˇerror warning info hint"});
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"error ˇwarning info hint"});
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"error warning ˇinfo hint"});
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"error warning info ˇhint"});
    go!(GoToDiagnosticSeverityFilter::default());
    cx.assert_editor_state(indoc! {"ˇerror warning info hint"});

    let only_info = GoToDiagnosticSeverityFilter::Only(GoToDiagnosticSeverity::Information);
    go!(only_info);
    cx.assert_editor_state(indoc! {"error warning ˇinfo hint"});
    go!(only_info);
    cx.assert_editor_state(indoc! {"error warning ˇinfo hint"});

    let no_hints = GoToDiagnosticSeverityFilter::Range {
        min: GoToDiagnosticSeverity::Information,
        max: GoToDiagnosticSeverity::Error,
    };

    go!(no_hints);
    cx.assert_editor_state(indoc! {"ˇerror warning info hint"});
    go!(no_hints);
    cx.assert_editor_state(indoc! {"error ˇwarning info hint"});
    go!(no_hints);
    cx.assert_editor_state(indoc! {"error warning ˇinfo hint"});
    go!(no_hints);
    cx.assert_editor_state(indoc! {"ˇerror warning info hint"});

    let warning_info = GoToDiagnosticSeverityFilter::Range {
        min: GoToDiagnosticSeverity::Information,
        max: GoToDiagnosticSeverity::Warning,
    };

    go!(warning_info);
    cx.assert_editor_state(indoc! {"error ˇwarning info hint"});
    go!(warning_info);
    cx.assert_editor_state(indoc! {"error warning ˇinfo hint"});
    go!(warning_info);
    cx.assert_editor_state(indoc! {"error ˇwarning info hint"});
}
