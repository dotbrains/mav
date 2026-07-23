use super::*;

impl EditPredictionStore {
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<EditPredictionStoreGlobal>()
            .map(|global| global.0.clone())
    }

    pub fn global(
        client: &Arc<Client>,
        user_store: &Entity<UserStore>,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.try_global::<EditPredictionStoreGlobal>()
            .map(|global| global.0.clone())
            .unwrap_or_else(|| {
                let ep_store = cx.new(|cx| Self::new(client.clone(), user_store.clone(), cx));
                cx.set_global(EditPredictionStoreGlobal(ep_store.clone()));
                ep_store
            })
    }

    pub fn new(client: Arc<Client>, user_store: Entity<UserStore>, cx: &mut Context<Self>) -> Self {
        let llm_token = global_llm_token(cx);
        let legacy_data_collection_enabled = Self::load_legacy_data_collection_enabled(cx);

        let (reject_tx, reject_rx) = mpsc::unbounded();
        cx.background_spawn({
            let client = client.clone();
            let llm_token = llm_token.clone();
            let app_version = AppVersion::global(cx);
            let background_executor = cx.background_executor().clone();
            async move {
                Self::handle_rejected_predictions(
                    reject_rx,
                    client,
                    llm_token,
                    app_version,
                    background_executor,
                )
                .await
            }
        })
        .detach();

        let (settled_predictions_tx, settled_predictions_rx) = mpsc::unbounded();
        cx.spawn({
            let client = client.clone();
            let llm_token = llm_token.clone();
            let app_version = AppVersion::global(cx);
            async move |this, cx| {
                Self::run_settled_predictions_worker(
                    this,
                    settled_predictions_rx,
                    client,
                    llm_token,
                    app_version,
                    cx,
                )
                .await;
            }
        })
        .detach();

        let mut current_user = user_store.read(cx).watch_current_user();
        let fetch_experiments_task = cx.spawn(async move |this, cx| {
            while current_user.borrow().is_none() {
                current_user.next().await;
            }

            this.update(cx, |this, cx| {
                if cx.is_staff() {
                    this.refresh_available_experiments(cx);
                }
            })
            .log_err();
        });

        let credentials_provider = mav_credentials_provider::global(cx);

        let this = Self {
            projects: HashMap::default(),
            client,
            user_store,
            llm_token,
            _fetch_experiments_task: fetch_experiments_task,
            update_required: false,
            edit_prediction_model: EditPredictionModel::Zeta,
            zeta2_raw_config: Self::zeta2_raw_config_from_env(),
            request_backoff_until: None,
            preferred_experiment: None,
            available_experiments: Vec::new(),
            mercury: Mercury::new(cx),
            legacy_data_collection_enabled,

            reject_predictions_tx: reject_tx,
            settled_predictions_tx,
            rated_predictions: Default::default(),
            rateable_predictions: Default::default(),
            #[cfg(test)]
            settled_event_callback: None,

            credentials_provider,
        };

        this
    }

    pub(crate) fn zeta2_raw_config_from_env() -> Option<Zeta2RawConfig> {
        let version_str = env::var("MAV_ZETA_FORMAT").ok()?;
        let format = ZetaFormat::parse(&version_str).ok()?;
        let model_id = env::var("MAV_ZETA_MODEL").ok();
        let environment = env::var("MAV_ZETA_ENVIRONMENT").ok();
        Some(Zeta2RawConfig {
            model_id,
            environment,
            format,
        })
    }

    pub fn set_edit_prediction_model(&mut self, model: EditPredictionModel) {
        self.edit_prediction_model = model;
    }

    pub fn set_zeta2_raw_config(&mut self, config: Zeta2RawConfig) {
        self.zeta2_raw_config = Some(config);
    }

    pub fn zeta2_raw_config(&self) -> Option<&Zeta2RawConfig> {
        self.zeta2_raw_config.as_ref()
    }

    pub(crate) fn back_off_requests_after_timeout(&mut self, cx: &mut Context<Self>) {
        self.request_backoff_until = Some(cx.background_executor().now() + REQUEST_TIMEOUT_BACKOFF);
        log::info!(
            "Backing off edit prediction requests for {:?} after Cloud timeout",
            REQUEST_TIMEOUT_BACKOFF
        );
    }

    pub(crate) fn request_backoff_active(&mut self, cx: &App) -> bool {
        let Some(backoff_until) = self.request_backoff_until else {
            return false;
        };

        if cx.background_executor().now() < backoff_until {
            true
        } else {
            self.request_backoff_until = None;
            false
        }
    }

    pub fn preferred_experiment(&self) -> Option<&str> {
        self.preferred_experiment.as_deref()
    }

    pub fn set_preferred_experiment(&mut self, experiment: Option<String>) {
        self.preferred_experiment = experiment;
    }

    pub fn available_experiments(&self) -> &[String] {
        &self.available_experiments
    }

    pub fn active_experiment(&self) -> Option<&str> {
        self.preferred_experiment.as_deref().or_else(|| {
            self.rateable_predictions
                .iter()
                .find_map(|p| p.model_version.as_ref())
                .and_then(|model_version| model_version.strip_prefix("zeta2:"))
        })
    }

    pub fn refresh_available_experiments(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let llm_token = self.llm_token.clone();
        let app_version = AppVersion::global(cx);
        let is_jumps_api = cx.has_flag::<EditPredictionJumpsFeatureFlag>();
        let organization_id = self
            .user_store
            .read(cx)
            .current_organization()
            .map(|organization| organization.id.clone());

        cx.spawn(async move |this, cx| {
            let experiments = cx
                .background_spawn(async move {
                    let organization_id =
                        organization_id.ok_or_else(|| anyhow!("No organization selected."))?;
                    let url = client.http_client().build_mav_llm_url(
                        "/edit_prediction_experiments",
                        &[("is_jumps_api", if is_jumps_api { "true" } else { "false" })],
                    )?;
                    let mut response = client
                        .authenticated_llm_request(&llm_token, organization_id, |token| {
                            Ok(http_client::Request::builder()
                                .method(Method::GET)
                                .uri(url.as_ref())
                                .header("Authorization", format!("Bearer {token}"))
                                .header(MAV_VERSION_HEADER_NAME, app_version.to_string())
                                .body(Default::default())?)
                        })
                        .await?;
                    if response.status().is_success() {
                        let mut body = Vec::new();
                        response.body_mut().read_to_end(&mut body).await?;
                        let experiments: Vec<String> = serde_json::from_slice(&body)?;
                        Ok(experiments)
                    } else {
                        let mut body = String::new();
                        response.body_mut().read_to_string(&mut body).await?;
                        anyhow::bail!(
                            "Failed to fetch experiments: {:?}\nBody: {}",
                            response.status(),
                            body
                        );
                    }
                })
                .await?;
            this.update(cx, |this, cx| {
                this.available_experiments = experiments;
                cx.notify();
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn icons(&self, cx: &App) -> edit_prediction_types::EditPredictionIconSet {
        use ui::IconName;
        match self.edit_prediction_model {
            EditPredictionModel::Mercury => {
                edit_prediction_types::EditPredictionIconSet::new(IconName::Inception)
            }
            EditPredictionModel::Zeta => {
                edit_prediction_types::EditPredictionIconSet::new(IconName::MavPredict)
                    .with_disabled(IconName::MavPredictDisabled)
                    .with_up(IconName::MavPredictUp)
                    .with_down(IconName::MavPredictDown)
                    .with_error(IconName::MavPredictError)
            }
            EditPredictionModel::Fim { .. } => {
                let settings = &all_language_settings(None, cx).edit_predictions;
                match settings.provider {
                    EditPredictionProvider::Ollama => {
                        edit_prediction_types::EditPredictionIconSet::new(IconName::AiOllama)
                    }
                    _ => {
                        edit_prediction_types::EditPredictionIconSet::new(IconName::AiOpenAiCompat)
                    }
                }
            }
        }
    }

    pub fn has_mercury_api_token(&self, cx: &App) -> bool {
        self.mercury.api_token.read(cx).has_key()
    }

    pub fn mercury_has_payment_required_error(&self) -> bool {
        self.mercury.has_payment_required_error()
    }

    pub fn clear_history(&mut self) {
        for project_state in self.projects.values_mut() {
            project_state.clear_history();
        }
    }
}
