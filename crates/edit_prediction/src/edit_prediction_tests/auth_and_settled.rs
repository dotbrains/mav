use super::*;

#[gpui::test]
async fn test_unauthenticated_without_custom_url_blocks_prediction_impl(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            "main.rs": "fn main() {\n    \n}\n"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;

    let request_count = Arc::new(std::sync::atomic::AtomicUsize::default());
    let http_client = FakeHttpClient::create({
        let request_count = request_count.clone();
        move |_req| {
            request_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                Ok(gpui::http_client::Response::builder()
                    .status(401)
                    .body("Unauthorized".into())
                    .unwrap())
            }
        }
    });

    let client =
        cx.update(|cx| client::Client::new(Arc::new(FakeSystemClock::new()), http_client, cx));
    let user_store = cx.update(|cx| cx.new(|cx| client::UserStore::new(client.clone(), cx)));
    cx.update(|cx| {
        RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
    });

    let ep_store = cx.new(|cx| EditPredictionStore::new(client, project.read(cx).user_store(), cx));

    let buffer = project
        .update(cx, |project, cx| {
            let path = project
                .find_project_path(path!("/project/main.rs"), cx)
                .unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let cursor = buffer.read_with(cx, |buffer, _| buffer.anchor_before(Point::new(1, 4)));
    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx)
    });
    cx.background_executor.run_until_parked();

    let completion_task = ep_store.update(cx, |ep_store, cx| {
        ep_store.set_edit_prediction_model(EditPredictionModel::Zeta);
        ep_store.request_prediction(
            &project,
            &buffer,
            cursor,
            PredictEditsRequestTrigger::Other,
            cx,
        )
    });

    assert!(completion_task.await.unwrap().is_none());
    assert_eq!(request_count.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[gpui::test]
async fn test_edit_prediction_settled(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());

    // Buffer with two clearly separated regions:
    //   Region A = lines 0-9   (offsets 0..50)
    //   Region B = lines 20-29 (offsets 105..155)
    // A big gap in between so edits in one region never overlap the other.
    let mut content = String::new();
    for i in 0..30 {
        content.push_str(&format!("line {i:02}\n"));
    }

    fs.insert_tree(
        "/root",
        json!({
            "foo.md": content.clone()
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

    type SettledEventRecord = (EditPredictionId, String);
    let settled_events: Arc<Mutex<Vec<SettledEventRecord>>> = Arc::new(Mutex::new(Vec::new()));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);

        let settled_events = settled_events.clone();
        ep_store.settled_event_callback = Some(Box::new(move |id, text| {
            settled_events.lock().push((id, text));
        }));
    });

    // --- Phase 1: edit in region A and enqueue prediction A ---

    buffer.update(cx, |buffer, cx| {
        // Edit at the start of line 0.
        buffer.edit(vec![(0..0, "ADDED ")], None, cx);
    });
    cx.run_until_parked();

    let snapshot_a = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let empty_edits: Arc<[(Range<Anchor>, Arc<str>)]> = Vec::new().into();
    let edit_preview_a = buffer
        .read_with(cx, |buffer, cx| {
            buffer.preview_edits(empty_edits.clone(), cx)
        })
        .await;

    // Region A: first 10 lines of the buffer.
    let editable_region_a = 0..snapshot_a.point_to_offset(Point::new(10, 0));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.enqueue_settled_prediction(
            EditPredictionId("prediction-a".into()),
            &project,
            &buffer,
            &snapshot_a,
            editable_region_a.clone(),
            &edit_preview_a,
            None,
            None,
            None,
            Duration::from_secs(0),
            cx,
        );
    });

    // --- Phase 2: repeatedly edit in region A to keep it unsettled ---

    // Let the worker process the channel message before we start advancing.
    cx.run_until_parked();

    for region_a_edit_offset in (5..).take(3) {
        // Edit inside region A (not at the boundary) so `last_edit_at` is
        // updated before the worker's next wake.
        buffer.update(cx, |buffer, cx| {
            buffer.edit(
                vec![(region_a_edit_offset..region_a_edit_offset, "x")],
                None,
                cx,
            );
        });
        cx.run_until_parked();

        cx.executor()
            .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE / 2);
        cx.run_until_parked();
        assert!(
            settled_events.lock().is_empty(),
            "no settled events should fire while region A is still being edited"
        );
    }

    // Still nothing settled.
    assert!(settled_events.lock().is_empty());

    // --- Phase 3: edit in distinct region B, enqueue prediction B ---
    // Advance a small amount so B's quiescence window starts later than A's,
    // but not so much that A settles (A's last edit was at the start of
    // iteration 3, and it needs a full Q to settle).
    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE / 4);
    cx.run_until_parked();
    assert!(settled_events.lock().is_empty());

    let snapshot_b = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let line_20_offset = snapshot_b.point_to_offset(Point::new(20, 0));

    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(line_20_offset..line_20_offset, "NEW ")], None, cx);
    });
    cx.run_until_parked();

    let snapshot_b2 = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let edit_preview_b = buffer
        .read_with(cx, |buffer, cx| buffer.preview_edits(empty_edits, cx))
        .await;
    let editable_region_b = line_20_offset..snapshot_b2.point_to_offset(Point::new(25, 0));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.enqueue_settled_prediction(
            EditPredictionId("prediction-b".into()),
            &project,
            &buffer,
            &snapshot_b2,
            editable_region_b.clone(),
            &edit_preview_b,
            None,
            None,
            None,
            Duration::from_secs(0),
            cx,
        );
    });

    cx.run_until_parked();
    assert!(
        settled_events.lock().is_empty(),
        "neither prediction should have settled yet"
    );

    // --- Phase 4: let enough time pass for region A to settle ---
    // A's last edit was at T_a (during the last loop iteration). The worker is
    // sleeping until T_a + Q. We advance just enough to reach that wake time
    // (Q/4 since we already advanced Q/4 in phase 3 on top of the loop's
    // 3*Q/2). At that point A has been quiet for Q and settles, but B was
    // enqueued only Q/4 ago and stays pending.
    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE / 4);
    cx.run_until_parked();

    {
        let events = settled_events.lock().clone();
        assert_eq!(
            events.len(),
            1,
            "prediction and capture_sample for A should have settled, got: {events:?}"
        );
        assert_eq!(events[0].0, EditPredictionId("prediction-a".into()));
    }

    // --- Phase 5: let more time pass for region B to settle ---
    // B's last edit was Q/4 before A settled. The worker rescheduled to
    // B's last_edit_at + Q, which is 3Q/4 from now.
    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE * 3 / 4);
    cx.run_until_parked();

    {
        let events = settled_events.lock().clone();
        assert_eq!(
            events.len(),
            2,
            "both prediction and capture_sample settled events should be emitted for each request, got: {events:?}"
        );
        assert_eq!(events[1].0, EditPredictionId("prediction-b".into()));
    }
}
