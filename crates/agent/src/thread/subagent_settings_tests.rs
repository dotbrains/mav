use super::*;
use gpui::TestAppContext;
use language_model::fake_provider::FakeLanguageModel;

fn setup_parent_with_subagents(
    cx: &mut TestAppContext,
    parent: &Entity<Thread>,
    count: usize,
) -> Vec<Entity<Thread>> {
    cx.update(|cx| {
        let mut subagents = Vec::new();
        for _ in 0..count {
            let subagent = cx.new(|cx| Thread::new_subagent(parent, cx));
            parent.update(cx, |thread, _cx| {
                thread.register_running_subagent(subagent.downgrade());
            });
            subagents.push(subagent);
        }
        subagents
    })
}

#[gpui::test]
async fn test_set_model_propagates_to_subagents(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 2);

    let new_model: Arc<dyn LanguageModel> = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "test-provider",
        "new-model",
        "New Model",
        false,
    ));

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_model(new_model, cx);
        });

        for subagent in &subagents {
            let subagent_model_id = subagent.read(cx).model().unwrap().id();
            assert_eq!(
                subagent_model_id.0.as_ref(),
                "new-model",
                "Subagent model should match parent model after set_model"
            );
        }
    });
}

#[gpui::test]
async fn test_set_summarization_model_propagates_to_subagents(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 2);

    let summary_model: Arc<dyn LanguageModel> = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "test-provider",
        "summary-model",
        "Summary Model",
        false,
    ));

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_summarization_model(Some(summary_model), cx);
        });

        for subagent in &subagents {
            let subagent_summary_id = subagent.read(cx).summarization_model().unwrap().id();
            assert_eq!(
                subagent_summary_id.0.as_ref(),
                "summary-model",
                "Subagent summarization model should match parent after set_summarization_model"
            );
        }
    });
}

#[gpui::test]
async fn test_set_thinking_enabled_propagates_to_subagents(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 2);

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_thinking_enabled(true, cx);
        });

        for subagent in &subagents {
            assert!(
                subagent.read(cx).thinking_enabled(),
                "Subagent thinking should be enabled after parent enables it"
            );
        }

        parent.update(cx, |thread, cx| {
            thread.set_thinking_enabled(false, cx);
        });

        for subagent in &subagents {
            assert!(
                !subagent.read(cx).thinking_enabled(),
                "Subagent thinking should be disabled after parent disables it"
            );
        }
    });
}

#[gpui::test]
async fn test_set_thinking_effort_propagates_to_subagents(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 2);

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_thinking_effort(Some("high".to_string()), cx);
        });

        for subagent in &subagents {
            assert_eq!(
                subagent.read(cx).thinking_effort().map(|s| s.as_str()),
                Some("high"),
                "Subagent thinking effort should match parent"
            );
        }

        parent.update(cx, |thread, cx| {
            thread.set_thinking_effort(None, cx);
        });

        for subagent in &subagents {
            assert_eq!(
                subagent.read(cx).thinking_effort(),
                None,
                "Subagent thinking effort should be None after parent clears it"
            );
        }
    });
}

#[gpui::test]
async fn test_subagent_inherits_settings_at_creation(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_speed(Speed::Fast, cx);
            thread.set_thinking_enabled(true, cx);
            thread.set_thinking_effort(Some("high".to_string()), cx);
            thread.set_profile(AgentProfileId("custom-profile".into()), cx);
        });
    });

    let subagents = setup_parent_with_subagents(cx, &parent, 1);

    cx.update(|cx| {
        let sub = subagents[0].read(cx);
        assert_eq!(sub.speed(), Some(Speed::Fast));
        assert!(sub.thinking_enabled());
        assert_eq!(sub.thinking_effort().map(|s| s.as_str()), Some("high"));
        assert_eq!(sub.profile(), &AgentProfileId("custom-profile".into()));
    });
}

#[gpui::test]
async fn test_set_speed_propagates_to_subagents(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 2);

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_speed(Speed::Fast, cx);
        });

        for subagent in &subagents {
            assert_eq!(
                subagent.read(cx).speed(),
                Some(Speed::Fast),
                "Subagent speed should match parent after set_speed"
            );
        }
    });
}

#[gpui::test]
async fn test_dropped_subagent_does_not_panic(cx: &mut TestAppContext) {
    let (parent, _event_stream) = tests::setup_thread_for_test(cx).await;
    let subagents = setup_parent_with_subagents(cx, &parent, 1);

    drop(subagents);

    cx.update(|cx| {
        parent.update(cx, |thread, cx| {
            thread.set_thinking_enabled(true, cx);
            thread.set_speed(Speed::Fast, cx);
            thread.set_thinking_effort(Some("high".to_string()), cx);
        });
    });
}
