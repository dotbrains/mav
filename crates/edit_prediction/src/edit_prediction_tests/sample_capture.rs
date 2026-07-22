use super::*;

const MIT_LICENSE: &str = indoc! {r#"
    MIT License

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.
"#};

async fn init_sample_capture_test(
    tree: serde_json::Value,
    cx: &mut TestAppContext,
) -> (
    Entity<EditPredictionStore>,
    RequestChannels,
    Entity<Project>,
    Entity<Buffer>,
) {
    let (ep_store, requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", tree).await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
    });
    cx.run_until_parked();
    (ep_store, requests, project, buffer)
}

/// Enqueues a settled prediction capture with injected sample data, as the
/// real request path would when data collection is enabled. Returns the
/// editable offset range.
async fn enqueue_sample_capture(
    ep_store: &Entity<EditPredictionStore>,
    project: &Entity<Project>,
    buffer: &Entity<Buffer>,
    id: &str,
    editable_point_range: Range<Point>,
    context: CapturedPredictionContext,
    prompt_history_boundary: Option<PromptHistoryBoundary>,
    navigation_history: VecDeque<RecentFile>,
    cx: &mut TestAppContext,
) -> Range<usize> {
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let editable_offset_range = snapshot.point_to_offset(editable_point_range.start)
        ..snapshot.point_to_offset(editable_point_range.end);
    let empty_edits: Arc<[(Range<Anchor>, Arc<str>)]> = Vec::new().into();
    let edit_preview = buffer
        .read_with(cx, |buffer, cx| buffer.preview_edits(empty_edits, cx))
        .await;
    ep_store.update(cx, |ep_store, cx| {
        ep_store.enqueue_settled_prediction(
            EditPredictionId(id.to_string().into()),
            project,
            buffer,
            &snapshot,
            editable_offset_range.clone(),
            &edit_preview,
            None,
            None,
            None,
            Duration::from_secs(0),
            cx,
        );
        let pending_capture = ep_store
            .projects
            .get_mut(&project.entity_id())
            .unwrap()
            .pending_prediction_captures
            .last_mut()
            .unwrap();
        pending_capture.can_collect_data = true;
        pending_capture.sample_data = Some(PendingPredictionCaptureSampleData {
            context_task: Task::ready(Ok(context)),
            editable_path: snapshot.file().unwrap().path().as_std_path().into(),
            editable_offset_range: editable_offset_range.clone(),
            next_edit_cursor_offset: None,
            future_edit_history_events: Vec::new(),
            navigation_history,
            edit_events_before_quiescence: 0,
            prompt_history_boundary,
        });
    });
    editable_offset_range
}

#[gpui::test]
async fn test_edit_prediction_settled_sends_sample_data_after_quiescence(cx: &mut TestAppContext) {
    let (ep_store, mut requests, project, buffer) = init_sample_capture_test(
        json!({
            "LICENSE": MIT_LICENSE,
            "foo.md": (0..60).map(|ix| format!("line {ix}\n")).collect::<String>(),
        }),
        cx,
    )
    .await;

    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(1, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "prompted ")], None, cx);
    });
    cx.run_until_parked();

    let boundary = ep_store.update(cx, |ep_store, _cx| {
        let project_state = ep_store.projects.get(&project.entity_id()).unwrap();
        PromptHistoryBoundary {
            first_event_seq: project_state
                .last_event
                .as_ref()
                .map_or(project_state.next_last_event_seq, |last_event| {
                    last_event.seq
                }),
            snapshot: project_state
                .last_event
                .as_ref()
                .map(|last_event| last_event.new_snapshot.clone()),
        }
    });
    let editable_offset_range = enqueue_sample_capture(
        &ep_store,
        &project,
        &buffer,
        "prediction-sample",
        Point::new(2, 0)..Point::new(3, 0),
        CapturedPredictionContext {
            repository_url: Some("https://example.com/repo.git".to_string()),
            revision: Some("abc123".to_string()),
            uncommitted_diff: Some("--- a/foo.md\n+++ b/foo.md\n".to_string()),
            buffer_diagnostics: vec![zeta_prompt::ActiveBufferDiagnostic {
                severity: Some(1),
                message: "sample diagnostic".to_string(),
                snippet: String::new(),
                snippet_buffer_row_range: 0..0,
                diagnostic_range_in_snippet: 0..0,
            }],
            editable_context: vec![zeta_prompt::RelatedFile {
                path: Path::new("foo.md").into(),
                max_row: 60,
                excerpts: vec![zeta_prompt::RelatedExcerpt {
                    row_range: 0..2,
                    text: "line 0\nline 1\n".into(),
                    order: 0,
                    context_source: zeta_prompt::ContextSource::CurrentFile,
                }],
                in_open_source_repo: true,
            }],
        },
        Some(boundary),
        VecDeque::from([RecentFile {
            path: Path::new("foo.md").into(),
            cursor_position: Some(3),
        }]),
        cx,
    )
    .await;
    let expected_next_edit_cursor_offset = editable_offset_range.start;

    // A second capture whose user has data collection disabled; its sample
    // and settled region must be redacted at send time.
    let boundary = ep_store.update(cx, |ep_store, _cx| {
        let project_state = ep_store.projects.get(&project.entity_id()).unwrap();
        PromptHistoryBoundary {
            first_event_seq: project_state
                .last_event
                .as_ref()
                .map_or(project_state.next_last_event_seq, |last_event| {
                    last_event.seq
                }),
            snapshot: project_state
                .last_event
                .as_ref()
                .map(|last_event| last_event.new_snapshot.clone()),
        }
    });
    enqueue_sample_capture(
        &ep_store,
        &project,
        &buffer,
        "prediction-redacted",
        Point::new(2, 0)..Point::new(3, 0),
        CapturedPredictionContext {
            repository_url: None,
            revision: None,
            uncommitted_diff: None,
            buffer_diagnostics: Vec::new(),
            editable_context: Vec::new(),
        },
        Some(boundary),
        VecDeque::new(),
        cx,
    )
    .await;
    ep_store.update(cx, |ep_store, _cx| {
        ep_store
            .projects
            .get_mut(&project.entity_id())
            .unwrap()
            .pending_prediction_captures
            .last_mut()
            .unwrap()
            .can_collect_data = false;
    });

    cx.executor().advance_clock(LAST_CHANGE_GROUPING_TIME * 2);
    cx.run_until_parked();

    for (ix, row) in [2, 20, 30, 40, 50].into_iter().enumerate() {
        buffer.update(cx, |buffer, cx| {
            let start = Point::new(row, 0).to_offset(buffer);
            let end = Point::new(row, 4).to_offset(buffer);
            buffer.edit(vec![(start..end, format!("future {ix}"))], None, cx);
        });
        cx.run_until_parked();
    }

    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE);
    cx.run_until_parked();

    let mut settled_by_id = std::collections::HashMap::new();
    for _ in 0..2 {
        let request = requests
            .settled
            .next()
            .await
            .expect("settled request should be sent");
        settled_by_id.insert(request.request_id.clone(), request);
    }

    let redacted_request = settled_by_id.remove("prediction-redacted").unwrap();
    assert!(!redacted_request.can_collect_data);
    assert_eq!(redacted_request.settled_editable_region, None);
    assert_eq!(redacted_request.sample_data, None);

    let settled_request = settled_by_id.remove("prediction-sample").unwrap();
    let sample_data = settled_request
        .sample_data
        .expect("sample data should be sent after quiescence");
    assert_eq!(
        sample_data.repository_url.as_deref(),
        Some("https://example.com/repo.git")
    );
    assert_eq!(sample_data.revision.as_deref(), Some("abc123"));
    assert_eq!(
        sample_data.uncommitted_diff.as_deref(),
        Some("--- a/foo.md\n+++ b/foo.md\n")
    );
    assert_eq!(sample_data.editable_path.as_ref(), Path::new("foo.md"));
    assert_eq!(sample_data.editable_offset_range, editable_offset_range);
    assert_eq!(sample_data.buffer_diagnostics.len(), 1);
    assert_eq!(sample_data.editable_context.len(), 1);
    let editable_context = &sample_data.editable_context[0];
    assert_eq!(editable_context.path.as_ref(), Path::new("foo.md"));
    assert_eq!(editable_context.excerpts.len(), 1);
    assert_eq!(
        editable_context.excerpts[0].context_source,
        zeta_prompt::ContextSource::CurrentFile
    );
    assert_eq!(sample_data.future_edit_history_events.len(), 4);
    assert_eq!(sample_data.navigation_history.len(), 1);
    assert_eq!(sample_data.edit_events_before_quiescence, 5);
    assert_eq!(
        sample_data.next_edit_cursor_offset,
        Some(expected_next_edit_cursor_offset)
    );

    let future_event_diffs = sample_data
        .future_edit_history_events
        .iter()
        .map(|event| match event.as_ref() {
            zeta_prompt::Event::BufferChange { diff, .. } => diff.as_str(),
        })
        .collect::<Vec<_>>();
    assert!(future_event_diffs.iter().all(|diff| {
        diff.lines()
            .all(|line| !line.starts_with("+prompted ") && line != "-line 1")
    }));
    assert!(
        future_event_diffs
            .iter()
            .any(|diff| diff.contains("future 0"))
    );
    assert!(
        !future_event_diffs
            .iter()
            .any(|diff| diff.contains("future 4"))
    );
}

#[gpui::test]
async fn test_edit_prediction_settled_sample_data_requires_observing_all_events_since_request(
    cx: &mut TestAppContext,
) {
    let (ep_store, mut requests, project, buffer) = init_sample_capture_test(
        json!({
            "LICENSE": MIT_LICENSE,
            "foo.md": (0..30).map(|ix| format!("line {ix}\n")).collect::<String>(),
        }),
        cx,
    )
    .await;

    // Two predictions are requested while no event is pending.
    let (boundary_observed, boundary_missed) = ep_store.update(cx, |ep_store, _cx| {
        let project_state = ep_store.projects.get(&project.entity_id()).unwrap();
        let first_event_seq = project_state
            .last_event
            .as_ref()
            .map_or(project_state.next_last_event_seq, |last_event| {
                last_event.seq
            });
        let snapshot = project_state
            .last_event
            .as_ref()
            .map(|last_event| last_event.new_snapshot.clone());
        (
            PromptHistoryBoundary {
                first_event_seq,
                snapshot: snapshot.clone(),
            },
            PromptHistoryBoundary {
                first_event_seq,
                snapshot,
            },
        )
    });
    assert!(boundary_observed.snapshot.is_none());

    // The first capture is enqueued immediately, so it observes both
    // subsequent events.
    enqueue_sample_capture(
        &ep_store,
        &project,
        &buffer,
        "prediction-observed",
        Point::new(1, 0)..Point::new(2, 0),
        CapturedPredictionContext {
            repository_url: None,
            revision: None,
            uncommitted_diff: None,
            buffer_diagnostics: Vec::new(),
            editable_context: Vec::new(),
        },
        Some(boundary_observed),
        VecDeque::new(),
        cx,
    )
    .await;

    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(1, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "first ")], None, cx);
    });
    cx.run_until_parked();
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(20, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "second ")], None, cx);
    });
    cx.run_until_parked();

    // The second capture is enqueued only after the first event was
    // finalized, so its future history has a gap and it must be dropped.
    enqueue_sample_capture(
        &ep_store,
        &project,
        &buffer,
        "prediction-missed",
        Point::new(1, 0)..Point::new(2, 0),
        CapturedPredictionContext {
            repository_url: None,
            revision: None,
            uncommitted_diff: None,
            buffer_diagnostics: Vec::new(),
            editable_context: Vec::new(),
        },
        Some(boundary_missed),
        VecDeque::new(),
        cx,
    )
    .await;

    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE);
    cx.run_until_parked();

    let mut settled_by_id = std::collections::HashMap::new();
    for _ in 0..2 {
        let request = requests
            .settled
            .next()
            .await
            .expect("settled request should be sent");
        settled_by_id.insert(request.request_id.clone(), request);
    }

    let observed_request = settled_by_id.remove("prediction-observed").unwrap();
    let sample_data = observed_request
        .sample_data
        .expect("sample data should be sent");
    assert_eq!(sample_data.edit_events_before_quiescence, 2);
    assert_eq!(sample_data.future_edit_history_events.len(), 2);

    let missed_request = settled_by_id.remove("prediction-missed").unwrap();
    assert_eq!(missed_request.sample_data, None);
}

#[gpui::test]
async fn test_edit_prediction_settled_drops_future_events_when_their_oss_status_is_unknown(
    cx: &mut TestAppContext,
) {
    let (ep_store, mut requests, project, buffer) = init_sample_capture_test(
        json!({ "foo.md": (0..30).map(|ix| format!("line {ix}\n")).collect::<String>() }),
        cx,
    )
    .await;

    enqueue_sample_capture(
        &ep_store,
        &project,
        &buffer,
        "prediction-non-oss",
        Point::new(1, 0)..Point::new(2, 0),
        CapturedPredictionContext {
            repository_url: None,
            revision: None,
            uncommitted_diff: None,
            buffer_diagnostics: Vec::new(),
            editable_context: Vec::new(),
        },
        None,
        VecDeque::new(),
        cx,
    )
    .await;

    // There is no LICENSE file, so this event's open-source status is unknown
    // and the sample must be dropped.
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(20, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "outside ")], None, cx);
    });
    cx.run_until_parked();

    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE);
    cx.run_until_parked();

    let settled_request = requests
        .settled
        .next()
        .await
        .expect("settled request should be sent");
    assert_eq!(settled_request.sample_data, None);
}
