use crate::context_server_store::*;

#[gpui::test]
async fn test_context_server_enabled_disabled(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";

    let server_1_id = ContextServerId(SERVER_1_ID.into());

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let executor = cx.executor();
    let store = project.read_with(cx, |project, _| project.context_server_store());
    store.update(cx, |store, _| {
        store.set_context_server_factory(Box::new(move |id, _| {
            Arc::new(ContextServer::new(
                id.clone(),
                Arc::new(create_fake_transport(id.0.to_string(), executor.clone())),
            ))
        }));
    });

    set_context_server_configuration(
        vec![(
            server_1_id.0.clone(),
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

    // Ensure that mcp-1 is stopped once it is disabled.
    {
        let _server_events = assert_server_events(
            &store,
            vec![(server_1_id.clone(), ContextServerStatus::Stopped)],
            cx,
        );
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Stdio {
                    enabled: false,
                    remote: false,
                    command: ContextServerCommand {
                        path: "somebinary".into(),
                        args: vec!["arg".to_string()],
                        env: None,
                        timeout: None,
                    },
                },
            )],
            cx,
        );

        cx.run_until_parked();
    }

    // Ensure that mcp-1 is started once it is enabled again.
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_1_id.clone(), ContextServerStatus::Starting),
                (server_1_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        set_context_server_configuration(
            vec![(
                server_1_id.0.clone(),
                settings::ContextServerSettingsContent::Stdio {
                    enabled: true,
                    remote: false,
                    command: ContextServerCommand {
                        path: "somebinary".into(),
                        args: vec!["arg".to_string()],
                        timeout: None,
                        env: None,
                    },
                },
            )],
            cx,
        );

        cx.run_until_parked();
    }
}

#[gpui::test]
async fn test_context_server_respects_disable_ai(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";

    let server_1_id = ContextServerId(SERVER_1_ID.into());

    // Set up SettingsStore with disable_ai: true in user settings BEFORE creating project
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        DisableAiSettings::register(cx);
        // Set disable_ai via user settings (not override_global) so it persists through recompute_values
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |content| {
                content.project.disable_ai = Some(SaturatingBool(true));
            });
        });
    });

    // Now create the project (ContextServerStore will see disable_ai = true)
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/test"), json!({"code.rs": ""})).await;
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

    let executor = cx.executor();
    let store = project.read_with(cx, |project, _| project.context_server_store());
    store.update(cx, |store, _| {
        store.set_context_server_factory(Box::new(move |id, _| {
            Arc::new(ContextServer::new(
                id.clone(),
                Arc::new(create_fake_transport(id.0.to_string(), executor.clone())),
            ))
        }));
    });

    set_context_server_configuration(
        vec![(
            server_1_id.0.clone(),
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
        )],
        cx,
    );

    cx.run_until_parked();

    // Verify that no server started because AI is disabled
    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            None,
            "Server should not start when disable_ai is true"
        );
    });

    // Enable AI and verify server starts
    {
        let _server_events = assert_server_events(
            &store,
            vec![
                (server_1_id.clone(), ContextServerStatus::Starting),
                (server_1_id.clone(), ContextServerStatus::Running),
            ],
            cx,
        );
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |content| {
                    content.project.disable_ai = Some(SaturatingBool(false));
                });
            });
        });
        cx.run_until_parked();
    }

    // Disable AI again and verify server stops
    {
        let _server_events = assert_server_events(
            &store,
            vec![(server_1_id.clone(), ContextServerStatus::Stopped)],
            cx,
        );
        cx.update(|cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |content| {
                    content.project.disable_ai = Some(SaturatingBool(true));
                });
            });
        });
        cx.run_until_parked();
    }

    // Verify server is stopped
    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            Some(ContextServerStatus::Stopped),
            "Server should be stopped when disable_ai is true"
        );
    });
}

#[gpui::test]
async fn test_context_server_refreshed_when_worktree_added(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";

    let server_1_id = ContextServerId(SERVER_1_ID.into());

    let (fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;
    fs.insert_tree(path!("/second"), json!({"other.rs": ""}))
        .await;

    let executor = cx.executor();
    let store = project.read_with(cx, |project, _| project.context_server_store());
    store.update(cx, |store, _| {
        store.set_context_server_factory(Box::new(move |id, _| {
            Arc::new(ContextServer::new(
                id.clone(),
                Arc::new(create_fake_transport(id.0.to_string(), executor.clone())),
            ))
        }));
    });

    set_context_server_configuration(
        vec![(
            server_1_id.0.clone(),
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
        )],
        cx,
    );

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

    // Witness that adding a worktree triggers the store to refresh available
    // servers (via `cx.notify` after `maintain_servers`). Without the
    // `WorktreeStoreEvent::WorktreeAdded` subscription in `ContextServerStore`,
    // this counter would remain zero.
    let notify_count = Rc::new(RefCell::new(0usize));
    let _notify_subscription = cx.update(|cx| {
        let count = notify_count.clone();
        cx.observe(&store, move |_, _| {
            *count.borrow_mut() += 1;
        })
    });

    {
        let _server_events = assert_server_events(&store, vec![], cx);
        let _ = project.update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/second"), true, cx)
        });
        cx.run_until_parked();
    }

    cx.update(|cx| {
        assert!(
            *notify_count.borrow() > 0,
            "Adding a worktree should trigger the context server store to refresh"
        );
        assert!(
            store.read(cx).server_ids().contains(&server_1_id),
            "Configured server list should still include the server after a worktree is added"
        );
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            Some(ContextServerStatus::Running),
            "Server should still be running after a worktree is added"
        );
    });
}

#[gpui::test]
async fn test_server_ids_includes_disabled_servers(cx: &mut TestAppContext) {
    const ENABLED_SERVER_ID: &str = "enabled-server";
    const DISABLED_SERVER_ID: &str = "disabled-server";

    let enabled_server_id = ContextServerId(ENABLED_SERVER_ID.into());
    let disabled_server_id = ContextServerId(DISABLED_SERVER_ID.into());

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let executor = cx.executor();
    let store = project.read_with(cx, |project, _| project.context_server_store());
    store.update(cx, |store, _| {
        store.set_context_server_factory(Box::new(move |id, _| {
            Arc::new(ContextServer::new(
                id.clone(),
                Arc::new(create_fake_transport(id.0.to_string(), executor.clone())),
            ))
        }));
    });

    // Configure one enabled and one disabled server
    set_context_server_configuration(
        vec![
            (
                enabled_server_id.0.clone(),
                settings::ContextServerSettingsContent::Stdio {
                    enabled: true,
                    remote: false,
                    command: ContextServerCommand {
                        path: "somebinary".into(),
                        args: vec![],
                        env: None,
                        timeout: None,
                    },
                },
            ),
            (
                disabled_server_id.0.clone(),
                settings::ContextServerSettingsContent::Stdio {
                    enabled: false,
                    remote: false,
                    command: ContextServerCommand {
                        path: "somebinary".into(),
                        args: vec![],
                        env: None,
                        timeout: None,
                    },
                },
            ),
        ],
        cx,
    );

    cx.run_until_parked();

    // Verify that server_ids includes both enabled and disabled servers
    cx.update(|cx| {
        let server_ids = store.read(cx).server_ids().to_vec();
        assert!(
            server_ids.contains(&enabled_server_id),
            "server_ids should include enabled server"
        );
        assert!(
            server_ids.contains(&disabled_server_id),
            "server_ids should include disabled server"
        );
    });

    // Verify that the enabled server is running and the disabled server is not
    cx.read(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&enabled_server_id),
            Some(ContextServerStatus::Running),
            "enabled server should be running"
        );
        // Disabled server should not be in the servers map (status returns None)
        // but should still be in server_ids
        assert_eq!(
            store.read(cx).status_for_server(&disabled_server_id),
            None,
            "disabled server should not have a status (not in servers map)"
        );
    });
}
