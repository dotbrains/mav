use super::*;

pub(super) fn model_response(
    request: &PredictEditsV3Request,
    diff_to_apply: &str,
) -> PredictEditsV3Response {
    let editable_range =
        zeta_prompt::excerpt_range_for_format(Default::default(), &request.input.excerpt_ranges).1;
    let excerpt = request.input.cursor_excerpt[editable_range.clone()].to_string();
    let new_excerpt = apply_diff_to_string(diff_to_apply, &excerpt).unwrap();

    PredictEditsV3Response {
        request_id: Uuid::new_v4().to_string(),
        editable_range,
        output: new_excerpt,
        cursor_offset: None,
        model_version: None,
    }
}

pub(super) fn empty_response() -> PredictEditsV3Response {
    PredictEditsV3Response {
        request_id: Uuid::new_v4().to_string(),
        editable_range: 0..0,
        output: String::new(),
        cursor_offset: None,
        model_version: None,
    }
}

pub(super) const REQUEST_TIMEOUT_RESPONSE_ID: &str = "__request_timeout__";

pub(super) fn request_timeout_response() -> PredictEditsV3Response {
    PredictEditsV3Response {
        request_id: REQUEST_TIMEOUT_RESPONSE_ID.to_string(),
        editable_range: 0..0,
        output: String::new(),
        cursor_offset: None,
        model_version: None,
    }
}

pub(super) fn prompt_from_request(request: &PredictEditsV3Request) -> String {
    zeta_prompt::format_zeta_prompt(&request.input, zeta_prompt::ZetaFormat::default())
        .expect("default zeta prompt formatting should succeed in edit prediction tests")
}

pub(super) fn assert_no_predict_request_ready<Request, Response>(
    requests: &mut mpsc::UnboundedReceiver<(Request, oneshot::Sender<Response>)>,
) {
    if requests.next().now_or_never().flatten().is_some() {
        panic!("Unexpected prediction request while throttled.");
    }
}

pub(super) struct RequestChannels {
    predict: mpsc::UnboundedReceiver<(
        PredictEditsV3Request,
        oneshot::Sender<PredictEditsV3Response>,
    )>,
    predict_v4: mpsc::UnboundedReceiver<(
        PredictEditsV4Request,
        oneshot::Sender<PredictEditsV4Response>,
    )>,
    reject: mpsc::UnboundedReceiver<(RejectEditPredictionsBody, oneshot::Sender<()>)>,
    settled: mpsc::UnboundedReceiver<SettledEditPrediction>,
}

pub(super) fn init_test_with_fake_client(
    cx: &mut TestAppContext,
) -> (Entity<EditPredictionStore>, RequestChannels) {
    init_test_with_fake_client_and_legacy_data_collection(cx, None)
}

pub(super) fn init_test_with_fake_client_and_legacy_data_collection(
    cx: &mut TestAppContext,
    legacy_data_collection_choice: Option<&str>,
) -> (Entity<EditPredictionStore>, RequestChannels) {
    let result = cx.update(move |cx| {
        cx.set_global(AppDatabase::test_new());
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        disable_jumps_feature_flag(cx);
        zlog::init_test();

        if let Some(legacy_data_collection_choice) = legacy_data_collection_choice {
            KeyValueStore::global(cx)
                .write_kvp(
                    MAV_PREDICT_DATA_COLLECTION_CHOICE.into(),
                    legacy_data_collection_choice.to_string(),
                )
                .now_or_never()
                .expect("legacy data collection write should complete immediately")
                .expect("legacy data collection write should succeed");
        }

        let (predict_req_tx, predict_req_rx) = mpsc::unbounded();
        let (predict_v4_req_tx, predict_v4_req_rx) = mpsc::unbounded();
        let (reject_req_tx, reject_req_rx) = mpsc::unbounded();
        let (settled_req_tx, settled_req_rx) = mpsc::unbounded();

        let http_client = FakeHttpClient::create({
            move |req| {
                let uri = req.uri().path().to_string();
                let content_encoding = req
                    .headers()
                    .get("Content-Encoding")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let mut body = req.into_body();
                let predict_req_tx = predict_req_tx.clone();
                let predict_v4_req_tx = predict_v4_req_tx.clone();
                let reject_req_tx = reject_req_tx.clone();
                let settled_req_tx = settled_req_tx.clone();
                async move {
                    let resp = match uri.as_str() {
                        "/client/llm_tokens" => serde_json::to_string(&json!({
                            "token": "test"
                        }))
                        .unwrap(),
                        "/predict_edits/v3" => {
                            let mut buf = Vec::new();
                            body.read_to_end(&mut buf).await.ok();
                            let decompressed = zstd::decode_all(&buf[..]).unwrap();
                            let req = serde_json::from_slice(&decompressed).unwrap();

                            let (res_tx, res_rx) = oneshot::channel::<PredictEditsV3Response>();
                            predict_req_tx.unbounded_send((req, res_tx)).unwrap();
                            let response = res_rx.await?;
                            if response.request_id == REQUEST_TIMEOUT_RESPONSE_ID {
                                return Ok(Response::builder()
                                    .status(http_client::http::StatusCode::REQUEST_TIMEOUT)
                                    .body(
                                        http_client::http::StatusCode::REQUEST_TIMEOUT
                                            .as_str()
                                            .into(),
                                    )
                                    .unwrap());
                            }
                            serde_json::to_string(&response).unwrap()
                        }
                        "/predict_edits/v4" => {
                            let mut buf = Vec::new();
                            body.read_to_end(&mut buf).await.ok();
                            let decompressed = zstd::decode_all(&buf[..]).unwrap();
                            let req = serde_json::from_slice(&decompressed).unwrap();

                            let (res_tx, res_rx) = oneshot::channel::<PredictEditsV4Response>();
                            predict_v4_req_tx.unbounded_send((req, res_tx)).unwrap();
                            let response = res_rx.await?;
                            if response.request_id == REQUEST_TIMEOUT_RESPONSE_ID {
                                return Ok(Response::builder()
                                    .status(http_client::http::StatusCode::REQUEST_TIMEOUT)
                                    .body(
                                        http_client::http::StatusCode::REQUEST_TIMEOUT
                                            .as_str()
                                            .into(),
                                    )
                                    .unwrap());
                            }
                            serde_json::to_string(&response).unwrap()
                        }
                        "/predict_edits/reject" => {
                            let mut buf = Vec::new();
                            body.read_to_end(&mut buf).await.ok();
                            let req = serde_json::from_slice(&buf).unwrap();

                            let (res_tx, res_rx) = oneshot::channel();
                            reject_req_tx.unbounded_send((req, res_tx)).unwrap();
                            serde_json::to_string(&res_rx.await?).unwrap()
                        }
                        "/predict_edits/settled" => {
                            let mut buf = Vec::new();
                            body.read_to_end(&mut buf).await.ok();
                            let body = if content_encoding.as_deref() == Some("zstd") {
                                zstd::decode_all(&buf[..]).unwrap()
                            } else {
                                buf
                            };
                            let req: SubmitEditPredictionSettledBatchBody =
                                serde_json::from_slice(&body).unwrap();
                            for prediction in req.predictions {
                                settled_req_tx.unbounded_send(prediction).unwrap();
                            }
                            serde_json::to_string(&SubmitEditPredictionSettledResponse {}).unwrap()
                        }
                        _ => {
                            panic!("Unexpected path: {}", uri)
                        }
                    };

                    Ok(Response::builder().body(resp.into()).unwrap())
                }
            }
        });

        let client = client::Client::new(Arc::new(FakeSystemClock::new()), http_client, cx);
        client.cloud_client().set_credentials(1, "test".into());

        let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
        language_model::init(cx);
        RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
        let ep_store = EditPredictionStore::global(&client, &user_store, cx);

        (
            ep_store,
            user_store,
            RequestChannels {
                predict: predict_req_rx,
                predict_v4: predict_v4_req_rx,
                reject: reject_req_rx,
                settled: settled_req_rx,
            },
        )
    });

    let (ep_store, user_store, channels) = result;
    set_test_organization(&user_store, cx);
    (ep_store, channels)
}

/// Configures a current organization on the given `UserStore` for tests.
///
/// The test client starts out signed out, which causes `UserStore` to clear the
/// current organization once that initial status is processed. This waits for
/// that to happen before configuring the organization, so it isn't subsequently
/// wiped out.
pub(super) fn set_test_organization(user_store: &Entity<UserStore>, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update(|cx| {
        user_store.update(cx, |store, cx| {
            store.set_current_organization_configuration_for_test(
                Arc::new(Organization {
                    id: OrganizationId("org_1".into()),
                    name: "Organization 1".into(),
                    is_personal: false,
                }),
                OrganizationConfiguration {
                    is_mav_model_provider_enabled: true,
                    is_agent_thread_feedback_enabled: true,
                    is_collaboration_enabled: true,
                    edit_prediction: OrganizationEditPredictionConfiguration {
                        is_enabled: true,
                        is_feedback_enabled: true,
                    },
                },
                cx,
            )
        });
    });
}
