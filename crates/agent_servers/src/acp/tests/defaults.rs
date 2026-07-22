use super::*;

#[gpui::test]
async fn settings_changes_refresh_active_connection_defaults(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
    });

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree("/", serde_json::json!({ "a": {} })).await;
    let project = project::Project::test(fs, [std::path::Path::new("/a")], cx).await;
    let harness = test_support::connect_fake_acp_connection(project, cx).await;

    cx.update(|cx| {
        AllAgentServersSettings::override_global(
            AllAgentServersSettings(HashMap::from_iter([(
                "test".to_string(),
                settings::CustomAgentServerSettings::Custom {
                    path: PathBuf::from("test-agent"),
                    args: Vec::new(),
                    env: HashMap::default(),
                    default_mode: Some("manual".to_string()),
                    default_config_options: HashMap::from_iter([(
                        "mode".to_string(),
                        AgentConfigOptionValue::from("manual"),
                    )]),
                    favorite_config_option_values: HashMap::default(),
                }
                .into(),
            )])),
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(
        harness.connection.defaults.mode(),
        Some(acp::SessionModeId::new("manual"))
    );
    assert_eq!(
        harness
            .connection
            .defaults
            .config_option("mode")
            .as_ref()
            .and_then(AgentConfigOptionValue::as_value_id),
        Some("manual"),
    );

    cx.update(|cx| {
        AllAgentServersSettings::override_global(AllAgentServersSettings(HashMap::default()), cx);
    });
    cx.run_until_parked();

    assert_eq!(harness.connection.defaults.mode(), None);
    assert_eq!(harness.connection.defaults.config_option("mode"), None);
}

#[gpui::test]
async fn default_config_options_skip_boolean_defaults_when_acp_beta_is_disabled(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| init_settings_with_acp_beta_override(false, cx));

    let (connection, set_config_requests) = connect_config_defaults_test_agent(cx).await;
    connection.defaults.set(
        None,
        HashMap::from_iter([
            (
                "web_search".to_string(),
                AgentConfigOptionValue::Boolean(true),
            ),
            ("mode".to_string(), AgentConfigOptionValue::from("manual")),
        ]),
    );
    let config_options = Rc::new(RefCell::new(vec![
        acp::SessionConfigOption::boolean("web_search", "Web Search", false),
        acp::SessionConfigOption::select(
            "mode",
            "Mode",
            "auto",
            vec![
                acp::SessionConfigSelectOption::new("auto", "Auto"),
                acp::SessionConfigSelectOption::new("manual", "Manual"),
            ],
        ),
    ]));

    let mut async_cx = cx.to_async();
    connection.apply_default_config_options(
        &acp::SessionId::new("session-config-defaults"),
        &config_options,
        &mut async_cx,
    );
    drop(async_cx);
    cx.run_until_parked();

    let requests = set_config_requests
        .lock()
        .expect("set config requests mutex poisoned");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].config_id, acp::SessionConfigId::new("mode"));
    assert_eq!(
        requests[0].value,
        acp::SessionConfigOptionValue::value_id("manual")
    );

    let options = config_options.borrow();
    assert!(
        matches!(&options[0].kind, acp::SessionConfigKind::Boolean(boolean) if !boolean.current_value)
    );
    assert!(
        matches!(&options[1].kind, acp::SessionConfigKind::Select(select) if select.current_value == acp::SessionConfigValueId::new("manual"))
    );
}

#[gpui::test]
async fn default_config_options_apply_boolean_defaults_when_acp_beta_is_enabled(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| init_settings_with_acp_beta_override(true, cx));

    let (connection, set_config_requests) = connect_config_defaults_test_agent(cx).await;
    connection.defaults.set(
        None,
        HashMap::from_iter([(
            "web_search".to_string(),
            AgentConfigOptionValue::Boolean(true),
        )]),
    );
    let config_options = Rc::new(RefCell::new(vec![acp::SessionConfigOption::boolean(
        "web_search",
        "Web Search",
        false,
    )]));

    let mut async_cx = cx.to_async();
    connection.apply_default_config_options(
        &acp::SessionId::new("session-config-defaults"),
        &config_options,
        &mut async_cx,
    );
    drop(async_cx);
    cx.run_until_parked();

    let requests = set_config_requests
        .lock()
        .expect("set config requests mutex poisoned");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].config_id,
        acp::SessionConfigId::new("web_search")
    );
    assert_eq!(
        requests[0].value,
        acp::SessionConfigOptionValue::boolean(true)
    );

    let options = config_options.borrow();
    assert!(
        matches!(&options[0].kind, acp::SessionConfigKind::Boolean(boolean) if boolean.current_value)
    );
}

fn init_settings_with_acp_beta_override(enabled: bool, cx: &mut App) {
    let mut store = settings::SettingsStore::test(cx);
    store.register_setting::<feature_flags::FeatureFlagsSettings>();
    store.update_user_settings(cx, |content| {
        content.feature_flags.get_or_insert_default().insert(
            AcpBetaFeatureFlag::NAME.to_string(),
            if enabled { "on" } else { "off" }.to_string(),
        );
    });
    cx.set_global(store);
    cx.update_flags(false, Vec::new());
}

async fn connect_config_defaults_test_agent(
    cx: &mut gpui::TestAppContext,
) -> (
    AcpConnection,
    Arc<Mutex<Vec<acp::SetSessionConfigOptionRequest>>>,
) {
    let set_config_requests = Arc::new(Mutex::new(Vec::new()));
    let (client_transport, agent_transport) = agent_client_protocol::Channel::duplex();

    cx.background_spawn(
        Agent
            .builder()
            .name("config-defaults-test-agent")
            .on_receive_request(
                {
                    let set_config_requests = set_config_requests.clone();
                    async move |req: acp::SetSessionConfigOptionRequest, responder, _cx| {
                        set_config_requests
                            .lock()
                            .expect("set config requests mutex poisoned")
                            .push(req);

                        responder.respond(acp::SetSessionConfigOptionResponse::new(Vec::new()))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(agent_transport),
    )
    .detach();

    let (connection_tx, connection_rx) = futures::channel::oneshot::channel();
    let client_io_task = cx.background_spawn(async move {
        Client
            .builder()
            .name("config-defaults-test-client")
            .connect_with(
                client_transport,
                move |connection: ConnectionTo<Agent>| async move {
                    connection_tx.send(connection).ok();
                    futures::future::pending::<Result<(), acp::Error>>().await
                },
            )
            .await
            .ok();
    });

    let client_conn = connection_rx
        .await
        .expect("failed to receive ACP connection");
    let sessions = Rc::new(RefCell::new(HashMap::default()));

    let connection = cx.update(|cx| {
        AcpConnection::new_for_test(
            client_conn,
            sessions,
            acp::AgentCapabilities::default(),
            WeakEntity::new_invalid(),
            client_io_task,
            Task::ready(()),
            cx,
        )
    });

    (connection, set_config_requests)
}
