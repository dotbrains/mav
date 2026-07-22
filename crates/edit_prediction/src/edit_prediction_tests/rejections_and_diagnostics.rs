use super::*;

#[gpui::test]
async fn test_rejections_flushing(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);

    ep_store.update(cx, |ep_store, cx| {
        ep_store.reject_prediction(
            EditPredictionId("test-1".into()),
            EditPredictionRejectReason::Discarded,
            false,
            None,
            None,
            cx,
        );
        ep_store.reject_prediction(
            EditPredictionId("test-2".into()),
            EditPredictionRejectReason::Canceled,
            true,
            None,
            None,
            cx,
        );
    });

    cx.executor().advance_clock(REJECT_REQUEST_DEBOUNCE);
    cx.run_until_parked();

    let (reject_request, respond_tx) = requests.reject.next().await.unwrap();
    respond_tx.send(()).unwrap();

    // batched
    assert_eq!(reject_request.rejections.len(), 2);
    assert_eq!(
        reject_request.rejections[0],
        EditPredictionRejection {
            request_id: "test-1".to_string(),
            reason: EditPredictionRejectReason::Discarded,
            was_shown: false,
            model_version: None,
            e2e_latency_ms: None
        }
    );
    assert_eq!(
        reject_request.rejections[1],
        EditPredictionRejection {
            request_id: "test-2".to_string(),
            reason: EditPredictionRejectReason::Canceled,
            was_shown: true,
            model_version: None,
            e2e_latency_ms: None
        }
    );

    // Reaching batch size limit sends without debounce
    ep_store.update(cx, |ep_store, cx| {
        for i in 0..70 {
            ep_store.reject_prediction(
                EditPredictionId(format!("batch-{}", i).into()),
                EditPredictionRejectReason::Discarded,
                false,
                None,
                None,
                cx,
            );
        }
    });

    // First MAX/2 items are sent immediately
    cx.run_until_parked();
    let (reject_request, respond_tx) = requests.reject.next().await.unwrap();
    respond_tx.send(()).unwrap();

    assert_eq!(reject_request.rejections.len(), 50);
    assert_eq!(reject_request.rejections[0].request_id, "batch-0");
    assert_eq!(reject_request.rejections[49].request_id, "batch-49");

    // Remaining items are debounced with the next batch
    cx.executor().advance_clock(Duration::from_secs(15));
    cx.run_until_parked();

    let (reject_request, respond_tx) = requests.reject.next().await.unwrap();
    respond_tx.send(()).unwrap();

    assert_eq!(reject_request.rejections.len(), 20);
    assert_eq!(reject_request.rejections[0].request_id, "batch-50");
    assert_eq!(reject_request.rejections[19].request_id, "batch-69");

    // Request failure
    ep_store.update(cx, |ep_store, cx| {
        ep_store.reject_prediction(
            EditPredictionId("retry-1".into()),
            EditPredictionRejectReason::Discarded,
            false,
            None,
            None,
            cx,
        );
    });

    cx.executor().advance_clock(REJECT_REQUEST_DEBOUNCE);
    cx.run_until_parked();

    let (reject_request, _respond_tx) = requests.reject.next().await.unwrap();
    assert_eq!(reject_request.rejections.len(), 1);
    assert_eq!(reject_request.rejections[0].request_id, "retry-1");
    // Simulate failure
    drop(_respond_tx);

    // Add another rejection
    ep_store.update(cx, |ep_store, cx| {
        ep_store.reject_prediction(
            EditPredictionId("retry-2".into()),
            EditPredictionRejectReason::Discarded,
            false,
            None,
            None,
            cx,
        );
    });

    cx.executor().advance_clock(REJECT_REQUEST_DEBOUNCE);
    cx.run_until_parked();

    // Retry should include both the failed item and the new one
    let (reject_request, respond_tx) = requests.reject.next().await.unwrap();
    respond_tx.send(()).unwrap();

    assert_eq!(reject_request.rejections.len(), 2);
    assert_eq!(reject_request.rejections[0].request_id, "retry-1");
    assert_eq!(reject_request.rejections[1].request_id, "retry-2");
}

#[gpui::test]
fn test_active_buffer_diagnostics_fetching(cx: &mut TestAppContext) {
    let diagnostic_marker: TextRangeMarker = ('«', '»').into();
    let search_range_marker: TextRangeMarker = ('[', ']').into();

    let (text, mut ranges) = marked_text_ranges_by(
        indoc! {r#"
            fn alpha() {
                let «first_value» = 1;
            }

            [fn beta() {
                let «second_value» = 2;
                let third_value = second_value + missing_symbol;
            }ˇ]

            fn gamma() {
                let «fourth_value» = missing_other_symbol;
            }
        "#},
        vec![diagnostic_marker.clone(), search_range_marker.clone()],
    );

    let diagnostic_ranges = ranges.remove(&diagnostic_marker).unwrap_or_default();
    let search_ranges = ranges.remove(&search_range_marker).unwrap_or_default();

    let buffer = cx.new(|cx| Buffer::local(&text, cx));

    buffer.update(cx, |buffer, cx| {
        let snapshot = buffer.snapshot();
        let diagnostics = DiagnosticSet::new(
            diagnostic_ranges
                .iter()
                .enumerate()
                .map(|(index, range)| DiagnosticEntry {
                    range: snapshot.offset_to_point_utf16(range.start)
                        ..snapshot.offset_to_point_utf16(range.end),
                    diagnostic: Diagnostic {
                        severity: match index {
                            0 => DiagnosticSeverity::WARNING,
                            1 => DiagnosticSeverity::ERROR,
                            _ => DiagnosticSeverity::HINT,
                        },
                        message: match index {
                            0 => "first warning".to_string(),
                            1 => "second error".to_string(),
                            _ => "third hint".to_string(),
                        },
                        group_id: index + 1,
                        is_primary: true,
                        source_kind: language::DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                }),
            &snapshot,
        );
        buffer.update_diagnostics(LanguageServerId(0), diagnostics, cx);
    });

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let search_range = snapshot.offset_to_point(search_ranges[0].start)
        ..snapshot.offset_to_point(search_ranges[0].end);

    let active_buffer_diagnostics = zeta::active_buffer_diagnostics(&snapshot, search_range, 5, 0);

    assert_eq!(
        active_buffer_diagnostics,
        vec![zeta_prompt::ActiveBufferDiagnostic {
            severity: Some(1),
            message: "second error".to_string(),
            snippet: "    let second_value = 2;".to_string(),
            snippet_buffer_row_range: 5..5,
            diagnostic_range_in_snippet: 8..20,
        }]
    );

    let active_buffer_diagnostics =
        zeta::active_buffer_diagnostics(&snapshot, Point::new(0, 0)..snapshot.max_point(), 5, 100);
    assert_eq!(
        active_buffer_diagnostics,
        vec![
            zeta_prompt::ActiveBufferDiagnostic {
                severity: Some(1),
                message: "second error".to_string(),
                snippet: String::new(),
                snippet_buffer_row_range: 5..5,
                diagnostic_range_in_snippet: 0..0,
            },
            zeta_prompt::ActiveBufferDiagnostic {
                severity: Some(2),
                message: "first warning".to_string(),
                snippet: String::new(),
                snippet_buffer_row_range: 1..1,
                diagnostic_range_in_snippet: 0..0,
            },
            zeta_prompt::ActiveBufferDiagnostic {
                severity: Some(4),
                message: "third hint".to_string(),
                snippet: String::new(),
                snippet_buffer_row_range: 10..10,
                diagnostic_range_in_snippet: 0..0,
            },
        ]
    );

    let buffer = cx.new(|cx| {
        Buffer::local(
            indoc! {"
                one
                two
                three
                four
                five
            "},
            cx,
        )
    });

    buffer.update(cx, |buffer, cx| {
        let snapshot = buffer.snapshot();
        let diagnostics = DiagnosticSet::new(
            vec![
                DiagnosticEntry {
                    range: text::PointUtf16::new(0, 0)..text::PointUtf16::new(0, 3),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::ERROR,
                        message: "row zero".to_string(),
                        group_id: 1,
                        is_primary: true,
                        source_kind: language::DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                },
                DiagnosticEntry {
                    range: text::PointUtf16::new(2, 0)..text::PointUtf16::new(2, 5),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::WARNING,
                        message: "row two".to_string(),
                        group_id: 2,
                        is_primary: true,
                        source_kind: language::DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                },
                DiagnosticEntry {
                    range: text::PointUtf16::new(4, 0)..text::PointUtf16::new(4, 4),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::INFORMATION,
                        message: "row four".to_string(),
                        group_id: 3,
                        is_primary: true,
                        source_kind: language::DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                },
            ],
            &snapshot,
        );
        buffer.update_diagnostics(LanguageServerId(0), diagnostics, cx);
    });

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());

    let active_buffer_diagnostics =
        zeta::active_buffer_diagnostics(&snapshot, Point::new(2, 0)..Point::new(4, 0), 3, 0);

    assert_eq!(
        active_buffer_diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.severity,
                diagnostic.message.clone(),
                diagnostic.snippet.clone(),
                diagnostic.snippet_buffer_row_range.clone(),
                diagnostic.diagnostic_range_in_snippet.clone(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                Some(2),
                "row two".to_string(),
                "three".to_string(),
                2..2,
                0..5,
            ),
            (
                Some(3),
                "row four".to_string(),
                "five".to_string(),
                4..4,
                0..4,
            ),
        ]
    );
}

#[gpui::test]
fn test_active_buffer_diagnostics_collection_limits(cx: &mut TestAppContext) {
    let text = (0..25)
        .map(|row| format!("line {row}\n"))
        .collect::<String>();
    let buffer = cx.new(|cx| Buffer::local(&text, cx));

    buffer.update(cx, |buffer, cx| {
        let snapshot = buffer.snapshot();
        let diagnostics = DiagnosticSet::new(
            (0..25)
                .map(|row| DiagnosticEntry {
                    range: text::PointUtf16::new(row, 0)..text::PointUtf16::new(row, 4),
                    diagnostic: Diagnostic {
                        severity: DiagnosticSeverity::ERROR,
                        message: format!("row {row}"),
                        group_id: row as usize,
                        is_primary: true,
                        source_kind: language::DiagnosticSourceKind::Pushed,
                        ..Diagnostic::default()
                    },
                })
                .collect::<Vec<_>>(),
            &snapshot,
        );
        buffer.update_diagnostics(LanguageServerId(0), diagnostics, cx);
    });

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let active_buffer_diagnostics =
        zeta::active_buffer_diagnostics(&snapshot, Point::new(0, 0)..Point::new(25, 0), 12, 0);

    assert_eq!(active_buffer_diagnostics.len(), 20);
    assert!(
        active_buffer_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "row 12")
    );
    assert!(
        active_buffer_diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message != "row 0" && diagnostic.message != "row 24")
    );

    let text = (0..300)
        .map(|row| format!("line {row} has some diagnostic context\n"))
        .collect::<String>();
    let long_message = "diagnostic message ".repeat(1000);
    let buffer = cx.new(|cx| Buffer::local(&text, cx));

    buffer.update(cx, |buffer, cx| {
        let snapshot = buffer.snapshot();
        let diagnostics = DiagnosticSet::new(
            vec![DiagnosticEntry {
                range: text::PointUtf16::new(150, 0)..text::PointUtf16::new(150, 4),
                diagnostic: Diagnostic {
                    severity: DiagnosticSeverity::ERROR,
                    message: long_message.clone(),
                    group_id: 1,
                    is_primary: true,
                    source_kind: language::DiagnosticSourceKind::Pushed,
                    ..Diagnostic::default()
                },
            }],
            &snapshot,
        );
        buffer.update_diagnostics(LanguageServerId(0), diagnostics, cx);
    });

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let active_buffer_diagnostics = zeta::active_buffer_diagnostics(
        &snapshot,
        Point::new(100, 0)..Point::new(200, 0),
        150,
        2000,
    );

    assert_eq!(active_buffer_diagnostics.len(), 1);
    assert!(
        active_buffer_diagnostics[0].message.len()
            <= crate::zeta::MAX_ACTIVE_BUFFER_DIAGNOSTIC_MESSAGE_TOKENS_TO_COLLECT * 3 + 2
    );
    assert!(active_buffer_diagnostics[0].message.len() < long_message.len());
    assert!(
        active_buffer_diagnostics[0].snippet.len()
            <= crate::zeta::MAX_ACTIVE_BUFFER_DIAGNOSTIC_SNIPPET_TOKENS_TO_COLLECT * 3 + 2
    );
    assert!(active_buffer_diagnostics[0].snippet.len() < text.len());
}

// Generate a model response that would apply the given diff to the active file.
