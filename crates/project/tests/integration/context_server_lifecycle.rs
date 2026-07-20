use crate::context_server_store::*;

#[gpui::test]
async fn test_context_server_maintain_servers_loop(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";
    const SERVER_2_ID: &str = "mcp-2";

    let server_1_id = ContextServerId(SERVER_1_ID.into());
    let server_2_id = ContextServerId(SERVER_2_ID.into());

    let fake_descriptor_1 = Arc::new(FakeContextServerDescriptor::new(SERVER_1_ID));

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let executor = cx.executor();
    let store = project.read_with(cx, |project, _| project.context_server_store());
    store.update(cx, |store, cx| {
        store.set_context_server_factory(Box::new(move |id, _| {
            Arc::new(ContextServer::new(
                id.clone(),
                Arc::new(create_fake_transport(id.0.to_string(), executor.clone())),
            ))
        }));
        store.registry().update(cx, |registry, cx| {
            registry.register_context_server_descriptor(SERVER_1_ID.into(), fake_descriptor_1, cx);
        });
    });

    set_context_server_configuration(
        vec![(
            server_1_id.0.clone(),
            settings::ContextServerSettingsContent::Extension {
                enabled: true,
                remote: false,
                settings: json!({
                    "somevalue": true
                }),
            },
        )],
        cx,
    );

    // Ensure that mcp-1 starts up
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_1_id.clone(), ContextServerStatus::Starting),
                (server_1_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        cx.run_until_parked();
    }

    // Ensure that mcp-1 is restarted when the configuration was changed
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_1_id.clone(), ContextServerStatus::Stopped),
                (server_1_id.clone(), ContextServerStatus::Starting),
                (server_1_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Extension {
                    enabled: true,
                    remote: false,
                    settings: json!({
                        "somevalue": false
                    }),
                },
            )],
            cx,
        );

        cx.run_until_parked();
    }

    // Ensure that mcp-1 is not restarted when the configuration was not changed
    {
        let _server_events = assert_server_events(&store, vec![], cx);
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Extension {
                    enabled: true,
                    remote: false,
                    settings: json!({
                        "somevalue": false
                    }),
                },
            )],
            cx,
        );

        cx.run_until_parked();
    }

    // Ensure that mcp-2 is started once it is added to the settings
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_2_id.clone(), ContextServerStatus::Starting),
                (server_2_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        set_context_server_configuration(
            vec![
                (
                    server_1_id.0.clone(),
                    settings::ContextServerSettingsContent::Extension {
                        enabled: true,
                        remote: false,
                        settings: json!({
                            "somevalue": false
                        }),
                    },
                ),
                (
                    server_2_id.0.clone(),
                    settings::ContextServerSettingsContent::Stdio {
                        enabled: true,
                        remote: false,
                        command: ContextServerCommand {
                            path: "somebinary".into(),
                            args: vec!["arg".to_string()],
                            env: None,
                            timeout: None,
                        },
                    },
                ),
            ],
            cx,
        );

        cx.run_until_parked();
    }

    // Ensure that mcp-2 is restarted once the args have changed
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_2_id.clone(), ContextServerStatus::Stopped),
                (server_2_id.clone(), ContextServerStatus::Starting),
                (server_2_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        set_context_server_configuration(
            vec![
                (
                    server_1_id.0.clone(),
                    settings::ContextServerSettingsContent::Extension {
                        enabled: true,
                        remote: false,
                        settings: json!({
                            "somevalue": false
                        }),
                    },
                ),
                (
                    server_2_id.0.clone(),
                    settings::ContextServerSettingsContent::Stdio {
                        enabled: true,
                        remote: false,
                        command: ContextServerCommand {
                            path: "somebinary".into(),
                            args: vec!["anotherArg".to_string()],
                            env: None,
                            timeout: None,
                        },
                    },
                ),
            ],
            cx,
        );

        cx.run_until_parked();
    }

    // Ensure that mcp-2 is removed once it is removed from the settings
    {
        let _server_events = assert_server_events(
            &store,
            vec![(server_2_id.clone(), ContextServerStatus::Stopped)],
            cx,
        );
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Extension {
                    enabled: true,
                    remote: false,
                    settings: json!({
                        "somevalue": false
                    }),
                },
            )],
            cx,
        );

        cx.run_until_parked();

        cx.update(|cx| {
            assert_eq!(store.read(cx).status_for_server(&server_2_id), None);
        });
    }

    // Ensure that nothing happens if the settings do not change
    {
        let _server_events = assert_server_events(&store, vec![], cx);
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Extension {
                    enabled: true,
                    remote: false,
                    settings: json!({
                        "somevalue": false
                    }),
                },
            )],
            cx,
        );

        cx.run_until_parked();

        cx.update(|cx| {
            assert_eq!(
                store.read(cx).status_for_server(&server_1_id),
                Some(ContextServerStatus::Running)
            );
            assert_eq!(store.read(cx).status_for_server(&server_2_id), None);
        });
    }
}
