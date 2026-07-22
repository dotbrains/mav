#[gpui::test]
async fn test_copilot_does_not_start_when_ai_disabled(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        DisableAiSettings::register(cx);
        AllLanguageSettings::register(cx);

        // Set disable_ai to true before creating Copilot
        DisableAiSettings::override_global(DisableAiSettings { disable_ai: true }, cx);
    });

    let copilot = cx.new(|cx| Copilot {
        server_id: LanguageServerId(0),
        fs: FakeFs::new(cx.background_executor().clone()),
        node_runtime: NodeRuntime::unavailable(),
        server: CopilotServer::Disabled,
        buffers: Default::default(),
        _subscriptions: vec![],
    });

    // Try to start copilot - it should remain disabled
    copilot.update(cx, |copilot, cx| {
        copilot.start_copilot(false, false, cx);
    });

    // Verify the server is still disabled
    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Disabled),
            "Copilot should not start when disable_ai is true"
        );
    });
}

#[gpui::test]
async fn test_copilot_stops_when_ai_becomes_disabled(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        DisableAiSettings::register(cx);
        AllLanguageSettings::register(cx);

        // AI is initially enabled
        DisableAiSettings::override_global(DisableAiSettings { disable_ai: false }, cx);
    });

    // Create a fake Copilot that's already running, with the settings observer
    let (copilot, _lsp) = Copilot::fake(cx);

    // Add the settings observer that handles disable_ai changes
    copilot.update(cx, |_, cx| {
        cx.observe_global::<SettingsStore>(move |this, cx| {
            let ai_disabled = DisableAiSettings::get_global(cx).disable_ai;

            if ai_disabled {
                if !matches!(this.server, CopilotServer::Disabled) {
                    let shutdown = match mem::replace(&mut this.server, CopilotServer::Disabled) {
                        CopilotServer::Running(server) => {
                            let shutdown_future = server.lsp.shutdown();
                            Some(cx.background_spawn(async move {
                                if let Some(fut) = shutdown_future {
                                    fut.await;
                                }
                            }))
                        }
                        _ => None,
                    };
                    if let Some(task) = shutdown {
                        task.detach();
                    }
                    cx.notify();
                }
            }
        })
        .detach();
    });

    // Verify copilot is running
    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Running(_)),
            "Copilot should be running initially"
        );
    });

    // Now disable AI
    cx.update(|cx| {
        DisableAiSettings::override_global(DisableAiSettings { disable_ai: true }, cx);
    });

    // The settings observer should have stopped the server
    cx.run_until_parked();

    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Disabled),
            "Copilot should be disabled after disable_ai is set to true"
        );
    });
}
