use super::*;

impl EditPredictionStore {
    pub fn request_prediction(
        &mut self,
        project: &Entity<Project>,
        active_buffer: &Entity<Buffer>,
        position: language::Anchor,
        trigger: PredictEditsRequestTrigger,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<EditPredictionResult>>> {
        self.request_prediction_internal(
            project.clone(),
            active_buffer.clone(),
            position,
            trigger,
            cx,
        )
    }

    fn request_prediction_internal(
        &mut self,
        project: Entity<Project>,
        active_buffer: Entity<Buffer>,
        position: language::Anchor,
        trigger: PredictEditsRequestTrigger,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<EditPredictionResult>>> {
        let is_cloud_zeta = matches!(self.edit_prediction_model, EditPredictionModel::Zeta)
            && !matches!(
                all_language_settings(None, cx).edit_predictions.provider,
                EditPredictionProvider::Ollama | EditPredictionProvider::OpenAiCompatibleApi
            );
        if is_cloud_zeta && !self.client.cloud_client().has_credentials() {
            return Task::ready(Ok(None));
        }

        if is_cloud_zeta && self.request_backoff_active(cx) {
            log::debug!(
                "Skipping Zeta edit prediction request while backing off after Cloud timeout"
            );
            return Task::ready(Ok(None));
        }

        self.get_or_init_project(&project, cx);
        let (stored_events, prompt_history_boundary, debug_tx) = {
            let project_state = self.projects.get(&project.entity_id()).unwrap();
            (
                project_state.events(cx),
                Some(PromptHistoryBoundary {
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
                }),
                project_state.debug_tx.clone(),
            )
        };
        let events: Vec<Arc<zeta_prompt::Event>> =
            stored_events.iter().map(|e| e.event.clone()).collect();

        let snapshot = active_buffer.read(cx).snapshot();
        let cursor_point = position.to_point(&snapshot);
        let diagnostic_search_start = cursor_point.row.saturating_sub(DIAGNOSTIC_LINES_RANGE);
        let diagnostic_search_end = cursor_point.row + DIAGNOSTIC_LINES_RANGE;
        let diagnostic_search_range =
            Point::new(diagnostic_search_start, 0)..Point::new(diagnostic_search_end, 0);

        let related_files = self.context_for_project(&project, cx);
        let allow_jump = is_cloud_zeta && cx.has_flag::<EditPredictionJumpsFeatureFlag>();
        let mode = match all_language_settings(snapshot.file(), cx).edit_predictions_mode() {
            EditPredictionsMode::Eager => PredictEditsMode::Eager,
            EditPredictionsMode::Subtle => PredictEditsMode::Subtle,
        };

        let buffer_id = active_buffer.read(cx).remote_id();
        let (repository_url, revision) = project
            .read(cx)
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer_id, cx)
            .map(|(repository, _)| {
                let snapshot = repository.read(cx).snapshot();
                (
                    snapshot
                        .remote_origin_url
                        .clone()
                        .or_else(|| snapshot.remote_upstream_url.clone()),
                    snapshot
                        .head_commit
                        .as_ref()
                        .map(|commit| commit.sha.to_string()),
                )
            })
            .unwrap_or_default();

        let is_staff_mav_repo = cx.is_staff()
            && repository_url
                .as_ref()
                .is_some_and(|url| is_mav_industries_repo(url));
        let is_open_source = is_staff_mav_repo
            || (snapshot
                .file()
                .map_or(false, |file| self.is_file_open_source(&project, file, cx))
                && events.iter().all(|event| event.in_open_source_repo())
                && related_files.iter().all(|file| file.in_open_source_repo));

        let can_collect_data = !cfg!(test)
            && is_open_source
            && self.is_data_collection_enabled(cx)
            && matches!(self.edit_prediction_model, EditPredictionModel::Zeta);
        let editable_context = allow_jump.then(|| {
            self.collect_editable_context(
                project.clone(),
                active_buffer.clone(),
                position,
                Vec::new(),
                vec![ContextSource::CurrentFile, ContextSource::EditHistory],
                cx,
            )
        });
        let inputs = EditPredictionModelInput {
            project: project.clone(),
            buffer: active_buffer,
            snapshot,
            position,
            events,
            related_files,
            editable_context,
            mode,
            trigger,
            diagnostic_search_range,
            debug_tx,
            can_collect_data,
            is_open_source,
            allow_jump,
        };

        let task = match self.edit_prediction_model {
            EditPredictionModel::Zeta => {
                let context_task = can_collect_data
                    .then(|| {
                        let editable_context_task = self.collect_editable_context(
                            inputs.project.clone(),
                            inputs.buffer.clone(),
                            inputs.position,
                            Vec::new(),
                            vec![ContextSource::CurrentFile, ContextSource::EditHistory],
                            cx,
                        );
                        capture_prediction_context(
                            inputs.project.clone(),
                            inputs.buffer.clone(),
                            inputs.position,
                            stored_events,
                            repository_url.clone(),
                            revision,
                            editable_context_task,
                            cx,
                        )
                    })
                    .flatten();
                zeta::request_prediction_with_zeta(
                    self,
                    inputs,
                    context_task,
                    prompt_history_boundary,
                    repository_url,
                    cx,
                )
            }
            EditPredictionModel::Fim { format } => fim::request_prediction(inputs, format, cx),
            EditPredictionModel::Mercury => {
                self.mercury
                    .request_prediction(inputs, self.credentials_provider.clone(), cx)
            }
        };

        task
    }

    async fn send_raw_llm_request(
        request: RawCompletionRequest,
        client: Arc<Client>,
        custom_url: Option<Arc<Url>>,
        llm_token: LlmApiToken,
        organization_id: Option<OrganizationId>,
        app_version: Version,
    ) -> Result<(RawCompletionResponse, Option<EditPredictionUsage>)> {
        let url = if let Some(custom_url) = custom_url {
            custom_url.as_ref().clone()
        } else {
            client
                .http_client()
                .build_mav_llm_url("/predict_edits/raw", &[])?
        };

        Self::send_api_request(
            |builder| {
                let req = builder
                    .uri(url.as_ref())
                    .body(serde_json::to_string(&request)?.into());
                Ok(req?)
            },
            client,
            llm_token,
            organization_id,
            app_version,
        )
        .await
    }

    pub(crate) async fn send_v3_request(
        input: Zeta2PromptInput,
        preferred_experiment: Option<String>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        organization_id: Option<OrganizationId>,
        app_version: Version,
        trigger: PredictEditsRequestTrigger,
        mode: PredictEditsMode,
    ) -> Result<(PredictEditsV3Response, Option<EditPredictionUsage>)> {
        let request = PredictEditsV3Request { input };
        Self::send_predict_edits_request(
            "/predict_edits/v3",
            request,
            preferred_experiment,
            client,
            llm_token,
            organization_id,
            app_version,
            trigger,
            mode,
        )
        .await
    }

    pub(crate) async fn send_v4_request(
        input: Zeta3PromptInput,
        preferred_experiment: Option<String>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        organization_id: Option<OrganizationId>,
        app_version: Version,
        trigger: PredictEditsRequestTrigger,
        mode: PredictEditsMode,
    ) -> Result<(PredictEditsV4Response, Option<EditPredictionUsage>)> {
        let request = PredictEditsV4Request { input };
        Self::send_predict_edits_request(
            "/predict_edits/v4",
            request,
            preferred_experiment,
            client,
            llm_token,
            organization_id,
            app_version,
            trigger,
            mode,
        )
        .await
    }

    async fn send_predict_edits_request<Req, Res>(
        path: &str,
        request: Req,
        preferred_experiment: Option<String>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        organization_id: Option<OrganizationId>,
        app_version: Version,
        trigger: PredictEditsRequestTrigger,
        mode: PredictEditsMode,
    ) -> Result<(Res, Option<EditPredictionUsage>)>
    where
        Req: serde::Serialize,
        Res: serde::de::DeserializeOwned,
    {
        let url = client.http_client().build_mav_llm_url(path, &[])?;
        let request_id = uuid::Uuid::new_v4().to_string();

        let json_bytes = serde_json::to_vec(&request)?;
        let compressed = zstd::encode_all(&json_bytes[..], 3)?;

        Self::send_api_request(
            |builder| {
                let builder = builder
                    .uri(url.as_ref())
                    .header("Content-Encoding", "zstd")
                    .header(PREDICT_EDITS_MODE_HEADER_NAME, mode.as_ref())
                    .header(PREDICT_EDITS_REQUEST_ID_HEADER_NAME, request_id.as_str())
                    .header(PREDICT_EDITS_TRIGGER_HEADER_NAME, trigger.as_ref());
                let builder = if let Some(preferred_experiment) = preferred_experiment.as_deref() {
                    builder.header(PREFERRED_EXPERIMENT_HEADER_NAME, preferred_experiment)
                } else {
                    builder
                };
                let req = builder.body(compressed.clone().into());
                Ok(req?)
            },
            client,
            llm_token,
            organization_id,
            app_version,
        )
        .await
    }

    async fn send_api_request<Res>(
        build: impl Fn(http_client::http::request::Builder) -> Result<http_client::Request<AsyncBody>>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        organization_id: Option<OrganizationId>,
        app_version: Version,
    ) -> Result<(Res, Option<EditPredictionUsage>)>
    where
        Res: DeserializeOwned,
    {
        let organization_id =
            organization_id.ok_or_else(|| anyhow!("No organization selected."))?;

        let response = client
            .authenticated_llm_request(&llm_token, organization_id, |token| {
                build(
                    http_client::Request::builder()
                        .method(Method::POST)
                        .header("Content-Type", "application/json")
                        .header(MAV_VERSION_HEADER_NAME, app_version.to_string())
                        .header("Authorization", format!("Bearer {token}")),
                )
            })
            .await?;

        Self::process_api_response(response, &app_version).await
    }

    async fn process_api_response<Res>(
        mut response: http_client::Response<AsyncBody>,
        app_version: &Version,
    ) -> Result<(Res, Option<EditPredictionUsage>)>
    where
        Res: DeserializeOwned,
    {
        if let Some(minimum_required_version) = response
            .headers()
            .get(MINIMUM_REQUIRED_VERSION_HEADER_NAME)
            .and_then(|version| Version::from_str(version.to_str().ok()?).ok())
        {
            anyhow::ensure!(
                *app_version >= minimum_required_version,
                MavUpdateRequiredError {
                    minimum_version: minimum_required_version
                }
            );
        }

        if response.status().is_success() {
            let usage = EditPredictionUsage::from_headers(response.headers()).ok();
            let mut body = Vec::new();
            response.body_mut().read_to_end(&mut body).await?;
            Ok((serde_json::from_slice(&body)?, usage))
        } else {
            let status = response.status();
            let mut body = String::new();
            response.body_mut().read_to_string(&mut body).await?;
            if status == http_client::http::StatusCode::REQUEST_TIMEOUT {
                return Err(anyhow::Error::new(CloudRequestTimeoutError));
            }
            anyhow::bail!("Request failed with status: {status:?}\nBody: {body}");
        }
    }
}
