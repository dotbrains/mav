use super::*;

#[gpui::test]
async fn test_current_state(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    cx.update(|cx| set_jumps_feature_flag_override(cx, "on"));
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "1.txt": "Hello!\nHow\nBye\n",
            "2.txt": "Hola!\nComo\nAdios\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer1 = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("/root/1.txt"), cx).unwrap();
            project.set_active_path(Some(path.clone()), cx);
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot1 = buffer1.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot1.anchor_before(language::Point::new(1, 3));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_project(&project, cx);
        ep_store.register_buffer(&buffer1, &project, cx);
    });

    // Prediction for current file

    ep_store.update(cx, |ep_store, cx| {
        ep_store.refresh_prediction_from_buffer(
            project.clone(),
            buffer1.clone(),
            position,
            EditPredictionRequestTrigger::Other,
            cx,
        )
    });
    let (_request, respond_tx) = requests.predict_v4.next().await.unwrap();

    respond_tx
        .send(PredictEditsV4Response {
            request_id: Uuid::new_v4().to_string(),
            patch: indoc! {r"
                --- a/root/1.txt
                +++ b/root/1.txt
                @@ ... @@
                 Hello!
                -How
                +How are you?
                 Bye
            "}
            .to_string(),
            model_version: None,
        })
        .unwrap();

    cx.run_until_parked();

    ep_store.update(cx, |ep_store, cx| {
        let prediction = ep_store
            .prediction_at(&buffer1, None, &project, cx)
            .unwrap();
        assert_matches!(prediction, BufferEditPrediction::Local { .. });
    });

    ep_store.update(cx, |ep_store, cx| {
        ep_store.reject_current_prediction(EditPredictionRejectReason::Discarded, &project, cx);
    });
}

#[gpui::test]
async fn test_refresh_prediction_from_buffer_suppressed_while_following(cx: &mut TestAppContext) {
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

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });
    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    cx.update(|cx| {
        AppState::set_global(workspace.read(cx).app_state().clone(), cx);
    });
    drop(app_state);

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

    multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.start_following(CollaboratorId::Agent, window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

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

    assert_no_predict_request_ready(&mut requests.predict);
}

#[gpui::test]
async fn test_simple_request(cx: &mut TestAppContext) {
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

    let prediction_task = ep_store.update(cx, |ep_store, cx| {
        ep_store.request_prediction(
            &project,
            &buffer,
            position,
            PredictEditsRequestTrigger::Other,
            cx,
        )
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();

    // TODO Put back when we have a structured request again
    // assert_eq!(
    //     request.excerpt_path.as_ref(),
    //     Path::new(path!("root/foo.md"))
    // );
    // assert_eq!(
    //     request.cursor_point,
    //     Point {
    //         line: Line(1),
    //         column: 3
    //     }
    // );

    respond_tx
        .send(model_response(
            &request,
            indoc! { r"
                --- a/root/foo.md
                +++ b/root/foo.md
                @@ ... @@
                 Hello!
                -How
                +How are you?
                 Bye
            "},
        ))
        .unwrap();

    let prediction = prediction_task.await.unwrap().unwrap().prediction;

    assert_eq!(prediction.edits.len(), 1);
    assert_eq!(
        prediction.edits[0].0.to_point(&snapshot).start,
        language::Point::new(1, 3)
    );
    assert_eq!(prediction.edits[0].1.as_ref(), " are you?");
}

#[gpui::test]
async fn test_zeta_request_sends_settled_body_when_data_collection_is_disabled(
    cx: &mut TestAppContext,
) {
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
    });

    let prediction_task = ep_store.update(cx, |ep_store, cx| {
        ep_store.request_prediction(
            &project,
            &buffer,
            position,
            PredictEditsRequestTrigger::Other,
            cx,
        )
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();
    assert!(!request.input.can_collect_data);
    respond_tx
        .send(model_response(&request, SIMPLE_DIFF))
        .unwrap();

    prediction_task.await.unwrap().unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(EDIT_PREDICTION_SETTLED_QUIESCENCE);
    cx.run_until_parked();

    let settled_request = requests
        .settled
        .next()
        .now_or_never()
        .flatten()
        .expect("settled request should be sent");
    assert!(!settled_request.can_collect_data);
    assert_eq!(settled_request.settled_editable_region, None);
    assert_eq!(settled_request.sample_data, None);
}

#[gpui::test]
async fn test_request_events(cx: &mut TestAppContext) {
    let (ep_store, mut requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md": "Hello!\n\nBye\n"
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

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(7..7, "How")], None, cx);
    });

    let snapshot = buffer.read_with(cx, |buffer, _cx| buffer.snapshot());
    let position = snapshot.anchor_before(language::Point::new(1, 3));

    let prediction_task = ep_store.update(cx, |ep_store, cx| {
        ep_store.request_prediction(
            &project,
            &buffer,
            position,
            PredictEditsRequestTrigger::Other,
            cx,
        )
    });

    let (request, respond_tx) = requests.predict.next().await.unwrap();

    let prompt = prompt_from_request(&request);
    assert!(
        prompt.contains(indoc! {"
        --- a/root/foo.md
        +++ b/root/foo.md
        @@ -1,3 +1,3 @@
         Hello!
        -
        +How
         Bye
    "}),
        "{prompt}"
    );

    respond_tx
        .send(model_response(
            &request,
            indoc! {r#"
                --- a/root/foo.md
                +++ b/root/foo.md
                @@ ... @@
                 Hello!
                -How
                +How are you?
                 Bye
        "#},
        ))
        .unwrap();

    let prediction = prediction_task.await.unwrap().unwrap().prediction;

    assert_eq!(prediction.edits.len(), 1);
    assert_eq!(prediction.edits[0].1.as_ref(), " are you?");
}
