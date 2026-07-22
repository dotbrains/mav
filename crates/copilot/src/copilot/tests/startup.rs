#[gpui::test]
async fn test_copilot_starts_when_ai_becomes_enabled(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        DisableAiSettings::register(cx);
        AllLanguageSettings::register(cx);

        // AI is initially disabled
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

    // Verify copilot is disabled initially
    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Disabled),
            "Copilot should be disabled initially"
        );
    });

    // Try to start - should fail because AI is disabled
    // Use check_edit_prediction_provider=false to skip provider check
    copilot.update(cx, |copilot, cx| {
        copilot.start_copilot(false, false, cx);
    });

    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Disabled),
            "Copilot should remain disabled when disable_ai is true"
        );
    });

    // Now enable AI
    cx.update(|cx| {
        DisableAiSettings::override_global(DisableAiSettings { disable_ai: false }, cx);
    });

    // Try to start again - should work now
    copilot.update(cx, |copilot, cx| {
        copilot.start_copilot(false, false, cx);
    });

    copilot.read_with(cx, |copilot, _| {
        assert!(
            matches!(copilot.server, CopilotServer::Starting { .. }),
            "Copilot should be starting after disable_ai is set to false"
        );
    });
}

fn init_test(cx: &mut TestAppContext) {
    zlog::init_test();

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}
