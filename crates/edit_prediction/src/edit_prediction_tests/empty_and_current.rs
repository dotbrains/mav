use super::*;

#[gpui::test]
async fn test_empty_prediction(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md":  "Hello!\nHow\nBye\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Explicit,
            cx,
        );
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();
    let mut response = model_response(&request, "");
    response.model_version = Some("zeta2:test-empty".to_string());
    let id = response.request_id.clone();
    respond_tx.send(response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        assert!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .is_none()
        );
        let shown_predictions = ep_store.rateable_predictions().collect::<Vec<_>>();
        assert_eq!(shown_predictions.len(), 1);
        assert_eq!(shown_predictions[0].id.to_string(), id);
        assert!(shown_predictions[0].edits.is_empty());
        assert!(shown_predictions[0].editable_range.is_some());
        assert!(matches!(
            shown_predictions[0].trigger,
            PredictEditsRequestTrigger::Explicit
        ));
    });

    // prediction is reported as rejected
    let (reject_request, _) = requests.reject.next().await.unwrap();

    assert_eq!(
        &reject_request.rejections,
        &[EditPredictionRejection {
            request_id: id.clone(),
            reason: EditPredictionRejectReason::Empty,
            was_shown: false,
            model_version: Some("zeta2:test-empty".to_string()),
            e2e_latency_ms: Some(0),
        }]
    );
    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE);
    cx.run_until_parked();

    let settled_request = requests
        .settled
        .next()
        .now_or_never()
        .flatten()
        .expect("empty prediction should still send settled request");
    assert_eq!(settled_request.request_id, id);
    assert_eq!(settled_request.settled_editable_region, None);
    assert_eq!(settled_request.sample_data, None);
}

#[gpui::test]
async fn test_interpolated_empty(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md":  "Hello!\nHow\nBye\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

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

    buffer.update(cx, |buffer, cx| {
        buffer.set_text("Hello!\nHow are you?\nBye", cx);
    });

    let mut response = model_response(&request, SIMPLE_DIFF);
    response.model_version = Some("zeta2:test-interpolated-empty".to_string());
    let id = response.request_id.clone();
    respond_tx.send(response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        assert!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .is_none()
        );
        let shown_predictions = ep_store.rateable_predictions().collect::<Vec<_>>();
        assert_eq!(shown_predictions.len(), 1);
        assert_eq!(shown_predictions[0].id.to_string(), id);
        assert!(shown_predictions[0].edits.is_empty());
        assert!(shown_predictions[0].editable_range.is_some());
    });

    // prediction is reported as rejected
    let (reject_request, _) = requests.reject.next().await.unwrap();

    assert_eq!(
        &reject_request.rejections,
        &[EditPredictionRejection {
            request_id: id,
            reason: EditPredictionRejectReason::InterpolatedEmpty,
            was_shown: false,
            model_version: Some("zeta2:test-interpolated-empty".to_string()),
            e2e_latency_ms: Some(0),
        }]
    );
}

const SIMPLE_DIFF: &str = indoc! { r"
    --- a/root/foo.md
    +++ b/root/foo.md
    @@ ... @@
     Hello!
    -How
    +How are you?
     Bye
"};

#[gpui::test]
async fn test_replace_current(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md":  "Hello!\nHow\nBye\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

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
    let first_response = model_response(&request, SIMPLE_DIFF);
    let first_id = first_response.request_id.clone();
    respond_tx.send(first_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            first_id
        );
    });

    // a second request is triggered
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
    let second_response = model_response(&request, SIMPLE_DIFF);
    let second_id = second_response.request_id.clone();
    respond_tx.send(second_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // second replaces first
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            second_id
        );
    });

    // first is reported as replaced
    let (reject_request, _) = requests.reject.next().await.unwrap();

    assert_eq!(
        &reject_request.rejections,
        &[EditPredictionRejection {
            request_id: first_id,
            reason: EditPredictionRejectReason::Replaced,
            was_shown: false,
            model_version: None,
            e2e_latency_ms: Some(0),
        }]
    );
}

#[gpui::test]
async fn test_current_preferred(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md":  "Hello!\nHow\nBye\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

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
    let first_response = model_response(&request, SIMPLE_DIFF);
    let first_id = first_response.request_id.clone();
    respond_tx.send(first_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            first_id
        );
    });

    // a second request is triggered
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
    // worse than current prediction
    let mut second_response = model_response(
        &request,
        indoc! { r"
            --- a/root/foo.md
            +++ b/root/foo.md
            @@ ... @@
             Hello!
            -How
            +How are
             Bye
        "},
    );
    second_response.model_version = Some("zeta2:test-current-preferred".to_string());
    let second_id = second_response.request_id.clone();
    respond_tx.send(second_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // first is preferred over second
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            first_id
        );
        let shown_prediction_ids = ep_store
            .rateable_predictions()
            .map(|prediction| prediction.id.to_string())
            .collect::<Vec<_>>();
        assert!(shown_prediction_ids.is_empty());
    });

    // second is reported as rejected
    let (reject_request, _) = requests.reject.next().await.unwrap();

    assert_eq!(
        &reject_request.rejections,
        &[EditPredictionRejection {
            request_id: second_id,
            reason: EditPredictionRejectReason::CurrentPreferred,
            was_shown: false,
            model_version: Some("zeta2:test-current-preferred".to_string()),
            e2e_latency_ms: Some(0),
        }]
    );
}
