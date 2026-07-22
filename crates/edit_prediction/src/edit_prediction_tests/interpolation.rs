use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_edit_prediction_basic_interpolation(cx: &mut TestAppContext) {
    let buffer = cx.new(|cx| Buffer::local("Lorem ipsum dolor", cx));
    let edits: Arc<[(Range<Anchor>, Arc<str>)]> = cx.update(|cx| {
        to_completion_edits([(2..5, "REM".into()), (9..11, "".into())], &buffer, cx).into()
    });

    let edit_preview = cx
        .read(|cx| buffer.read(cx).preview_edits(edits.clone(), cx))
        .await;

    let prediction = EditPrediction {
        edits,
        cursor_position: None,
        editable_range: None,
        edit_preview,
        buffer: buffer.clone(),
        snapshot: cx.read(|cx| buffer.read(cx).snapshot()),
        id: EditPredictionId("the-id".into()),
        inputs: EditPredictionInputs::V2(Zeta2PromptInput {
            events: Default::default(),
            related_files: Default::default(),
            active_buffer_diagnostics: vec![],
            cursor_path: Path::new("").into(),
            cursor_excerpt: "".into(),
            cursor_offset_in_excerpt: 0,
            excerpt_start_row: None,
            excerpt_ranges: Default::default(),
            syntax_ranges: None,
            in_open_source_repo: false,
            can_collect_data: false,
            repo_url: None,
        }),
        model_version: None,
        trigger: PredictEditsRequestTrigger::Other,
    };

    cx.update(|cx| {
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(2..5, "REM".into()), (9..11, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(2..5, "")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(2..2, "REM".into()), (6..8, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.undo(cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(2..5, "REM".into()), (9..11, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(2..5, "R")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(3..3, "EM".into()), (7..9, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(3..3, "E")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(4..4, "M".into()), (8..10, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(4..4, "M")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(9..11, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(4..5, "")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(4..4, "M".into()), (8..10, "".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(8..10, "")], None, cx));
        assert_eq!(
            from_completion_edits(
                &prediction.interpolate(&buffer.read(cx).snapshot()).unwrap(),
                &buffer,
                cx
            ),
            vec![(4..4, "M".into())]
        );

        buffer.update(cx, |buffer, cx| buffer.edit([(4..6, "")], None, cx));
        assert_eq!(prediction.interpolate(&buffer.read(cx).snapshot()), None);
    })
}

#[gpui::test]
async fn test_clean_up_diff(cx: &mut TestAppContext) {
    init_test(cx);

    assert_eq!(
        apply_edit_prediction(
            indoc! {"
                    fn main() {
                        let word_1 = \"lorem\";
                        let range = word.len()..word.len();
                    }
                "},
            indoc! {"
                    fn main() {
                        let word_1 = \"lorem\";
                        let range = word_1.len()..word_1.len();
                    }
                "},
            cx,
        )
        .await,
        indoc! {"
                fn main() {
                    let word_1 = \"lorem\";
                    let range = word_1.len()..word_1.len();
                }
            "},
    );

    assert_eq!(
        apply_edit_prediction(
            indoc! {"
                    fn main() {
                        let story = \"the quick\"
                    }
                "},
            indoc! {"
                    fn main() {
                        let story = \"the quick brown fox jumps over the lazy dog\";
                    }
                "},
            cx,
        )
        .await,
        indoc! {"
                fn main() {
                    let story = \"the quick brown fox jumps over the lazy dog\";
                }
            "},
    );
}

#[gpui::test]
async fn test_edit_prediction_end_of_buffer(cx: &mut TestAppContext) {
    init_test(cx);

    let buffer_content = "lorem\n";
    let completion_response = "lorem\nipsum\n";

    assert_eq!(
        apply_edit_prediction(buffer_content, completion_response, cx).await,
        "lorem\nipsum\n"
    );
}

#[gpui::test]
async fn test_edit_prediction_v4_end_of_buffer(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            "file.txt": "lorem\n"
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            let path = project
                .find_project_path(path!("/project/file.txt"), cx)
                .unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let (ep_store, response) = make_test_ep_store(&project, cx).await;
    *response.lock() = "lorem\nipsum\n".to_string();

    let position = buffer.read_with(cx, |buffer, _| buffer.anchor_before(Point::new(1, 0)));
    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_project(&project, cx);
        ep_store.register_buffer(&buffer, &project, cx);
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });
    cx.run_until_parked();

    let edits = ep_store.update(cx, |ep_store, cx| {
        let prediction = ep_store
            .prediction_at(&buffer, None, &project, cx)
            .expect("should have prediction");
        let prediction = match prediction {
            BufferEditPrediction::Local { prediction }
            | BufferEditPrediction::Jump { prediction } => prediction,
        };
        assert!(prediction.editable_range.is_some());
        prediction.edits.iter().cloned().collect::<Vec<_>>()
    });
    buffer.update(cx, |buffer, cx| buffer.edit(edits, None, cx));

    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "lorem\nipsum\n");
    });
}

#[gpui::test]
async fn test_edit_prediction_no_spurious_trailing_newline(cx: &mut TestAppContext) {
    // Test that zeta2's newline normalization logic doesn't insert spurious newlines.
    // When the buffer ends without a trailing newline, but the model returns output
    // with a trailing newline, zeta2 should normalize both sides before diffing
    // so no spurious newline is inserted.
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());

    // Single line buffer with no trailing newline
    fs.insert_tree(
        "/root",
        json!({
            "foo.txt": "hello"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project
                .find_project_path(path!("root/foo.txt"), cx)
                .unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(0, 5));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();

    // Model returns output WITH a trailing newline, even though the buffer doesn't have one.
    // Zeta2 should normalize both sides before diffing, so no spurious newline is inserted.
    let excerpt_length = request.input.cursor_excerpt.len();
    let response = PredictEditsV3Response {
        request_id: Uuid::new_v4().to_string(),
        output: "hello world\n".to_string(),
        editable_range: 0..excerpt_length,
        model_version: None,
        cursor_offset: None,
    };
    respond_tx.send(response).unwrap();

    cx.run_until_parked();

    // The prediction should insert " world" without adding a newline
    ep_store.update(cx, |ep_store, cx| {
        let prediction = ep_store
            .prediction_at(&buffer, None, &project, cx)
            .expect("should have prediction");
        let edits: Vec<_> = prediction
            .edits
            .iter()
            .map(|(range, text)| {
                let snapshot = buffer.read(cx).snapshot();
                (range.to_offset(&snapshot), text.clone())
            })
            .collect();
        assert_eq!(edits, vec![(5..5, " world".into())]);
    });
}

#[gpui::test]
async fn test_v3_prediction_strips_cursor_marker_from_edit_text(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/root",
        json!({
            "foo.txt": "hello"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project
                .find_project_path(path!("root/foo.txt"), cx)
                .unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(0, 5));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();
    let excerpt_length = request.input.cursor_excerpt.len();
    respond_tx
        .send(PredictEditsV3Response {
            request_id: Uuid::new_v4().to_string(),
            output: "hello world".to_string(),
            editable_range: 0..excerpt_length,
            model_version: None,
            cursor_offset: Some(5),
        })
        .unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        let prediction = ep_store
            .prediction_at(&buffer, None, &project, cx)
            .expect("should have prediction");
        let snapshot = buffer.read(cx).snapshot();
        let edits: Vec<_> = prediction
            .edits
            .iter()
            .map(|(range, text)| (range.to_offset(&snapshot), text.clone()))
            .collect();

        assert_eq!(edits, vec![(5..5, " world".into())]);
    });
}
