use super::*;

#[gpui::test]
async fn test_cancel_earlier_pending_requests(cx: &mut TestAppContext) {
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

    // start two refresh tasks
    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request1, respond_first) = requests.predict.next().await.unwrap();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request, respond_second) = requests.predict.next().await.unwrap();

    // wait for throttle
    cx.run_until_parked();

    // second responds first
    let second_response = model_response(&request, SIMPLE_DIFF);
    let second_id = second_response.request_id.clone();
    respond_second.send(second_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // current prediction is second
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            second_id
        );
    });

    let mut first_response = model_response(&request1, SIMPLE_DIFF);
    first_response.model_version = Some("zeta2:test-canceled".to_string());
    let first_id = first_response.request_id.clone();
    respond_first.send(first_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // current prediction is still second, since first was cancelled
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            second_id
        );
    });

    // first is reported as rejected
    let (reject_request, _) = requests.reject.next().await.unwrap();

    cx.run_until_parked();

    assert_eq!(
        &reject_request.rejections,
        &[EditPredictionRejection {
            request_id: first_id,
            reason: EditPredictionRejectReason::Canceled,
            was_shown: false,
            model_version: Some("zeta2:test-canceled".to_string()),
            e2e_latency_ms: None,
        }]
    );
}

#[gpui::test]
async fn test_cancel_second_on_third_request(cx: &mut TestAppContext) {
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

    // start two refresh tasks
    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request1, respond_first) = requests.predict.next().await.unwrap();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    let (request2, respond_second) = requests.predict.next().await.unwrap();

    // wait for throttle, so requests are sent
    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // start a third request
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );

        // 2 are pending, so 2nd is cancelled
        assert_eq!(
            ep_store
                .get_or_init_project(&project, cx)
                .cancelled_predictions
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [1]
        );
    });

    // wait for throttle
    cx.run_until_parked();

    let (request3, respond_third) = requests.predict.next().await.unwrap();

    let first_response = model_response(&request1, SIMPLE_DIFF);
    let first_id = first_response.request_id.clone();
    respond_first.send(first_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // current prediction is first
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            first_id
        );
    });

    let mut cancelled_response = model_response(&request2, SIMPLE_DIFF);
    cancelled_response.model_version = Some("zeta2:test-canceled-second".to_string());
    let cancelled_id = cancelled_response.request_id.clone();
    respond_second.send(cancelled_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // current prediction is still first, since second was cancelled
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            first_id
        );
    });

    let third_response = model_response(&request3, SIMPLE_DIFF);
    let third_response_id = third_response.request_id.clone();
    respond_third.send(third_response).unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        // third completes and replaces first
        assert_eq!(
            ep_store
                .prediction_at(&buffer, None, &project, cx)
                .unwrap()
                .id
                .0,
            third_response_id
        );
    });

    // second is reported as rejected
    let (reject_request, _) = requests.reject.next().await.unwrap();

    cx.run_until_parked();

    assert_eq!(
        &reject_request.rejections,
        &[
            EditPredictionRejection {
                request_id: cancelled_id,
                reason: EditPredictionRejectReason::Canceled,
                was_shown: false,
                model_version: Some("zeta2:test-canceled-second".to_string()),
                e2e_latency_ms: None,
            },
            EditPredictionRejection {
                request_id: first_id,
                reason: EditPredictionRejectReason::Replaced,
                was_shown: false,
                model_version: None,
                // 2 throttle waits (for 2nd and 3rd requests) elapsed
                // between this request's start and response.
                e2e_latency_ms: Some(2 * EditPredictionStore::THROTTLE_TIMEOUT.as_millis()),
            }
        ]
    );
}

#[gpui::test]
async fn test_cloud_timeout_backs_off_zeta_requests(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md": "Hello!\nHow\nBye\n"
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
        ep_store.register_project(&project, cx);
        ep_store.register_buffer(&buffer, &project, cx);
    });

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });
    let (_request, respond_tx) = requests.predict.next().await.unwrap();
    respond_tx.send(request_timeout_response()).unwrap();
    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });
    cx.background_executor
        .advance_clock(EditPredictionStore::THROTTLE_TIMEOUT);
    cx.background_executor.run_until_parked();
    cx.run_until_parked();
    assert_no_predict_request_ready(&mut requests.predict);

    cx.background_executor
        .advance_clock(REQUEST_TIMEOUT_BACKOFF);
    cx.background_executor.run_until_parked();
    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });
    let (_request, respond_tx) = requests.predict.next().await.unwrap();
    respond_tx.send(empty_response()).unwrap();
    cx.run_until_parked();
}

#[gpui::test]
async fn test_same_frame_duplicate_requests_deduplicated(cx: &mut TestAppContext) {
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

    // Enqueue two refresh calls in the same synchronous frame (no yielding).
    // Both `cx.spawn` tasks are created before either executes, so they both
    // capture the same `proceed_count_at_enqueue`. Only the first task should
    // pass the deduplication gate; the second should be skipped.
    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        );
    });

    // Let both spawned tasks run to completion (including any throttle waits).
    cx.run_until_parked();

    // Exactly one prediction request should have been sent.
    let (request, respond_tx) = requests.predict.next().await.unwrap();
    respond_tx
        .send(model_response(&request, SIMPLE_DIFF))
        .unwrap();
    cx.run_until_parked();

    // No second request should be pending.
    assert_no_predict_request_ready(&mut requests.predict);
}
