use super::*;

#[gpui::test]
async fn test_create_thread_with_options_retains_thread_and_restores_agent(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;
    let _stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());

    panel.update(&mut cx, |panel, _cx| {
        panel.selected_agent = Agent::Stub;
    });

    let no_override_id = panel.update_in(&mut cx, |panel, window, cx| {
        panel.create_thread_with_options(
            CreateThreadOptions::default(),
            AgentThreadSource::AgentPanel,
            window,
            cx,
        )
    });

    panel.read_with(&cx, |panel, _cx| {
        assert!(
            panel.retained_threads.contains_key(&no_override_id),
            "thread created via create_thread_with_options should be retained"
        );
        assert_eq!(
            panel.selected_agent,
            Agent::Stub,
            "selected_agent should be unchanged when no agent override is requested"
        );
    });

    let override_agent = Agent::Custom {
        id: "override-agent".into(),
    };
    let override_id = panel.update_in(&mut cx, |panel, window, cx| {
        panel.create_thread_with_options(
            CreateThreadOptions {
                agent: Some(override_agent.clone()),
                ..CreateThreadOptions::default()
            },
            AgentThreadSource::AgentPanel,
            window,
            cx,
        )
    });

    panel.read_with(&cx, |panel, _cx| {
        assert!(
            panel.retained_threads.contains_key(&override_id),
            "thread created with an agent override should also be retained"
        );
        assert_ne!(
            no_override_id, override_id,
            "each call should produce a distinct ThreadId"
        );
        assert_eq!(
            panel.selected_agent,
            Agent::Stub,
            "selected_agent should be restored to the original after an agent override"
        );
    });
}
