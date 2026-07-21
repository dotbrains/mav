#[gpui::test]
async fn test_listing_models(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {}  })).await;
    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let connection = NativeAgentConnection(
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx)),
    );

    // Create a thread/session
    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/a")]),
                cx,
            )
        })
        .await
        .unwrap();

    let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

    let models = cx
        .update(|cx| {
            connection
                .model_selector(&session_id)
                .unwrap()
                .list_models(cx)
        })
        .await
        .unwrap();

    let acp_thread::AgentModelList::Grouped(models) = models else {
        panic!("Unexpected model group");
    };
    assert_eq!(
        models,
        IndexMap::from_iter([(
            AgentModelGroupName("Fake".into()),
            vec![AgentModelInfo {
                id: AgentModelId::new("fake/fake"),
                name: "Fake".into(),
                description: None,
                icon: Some(acp_thread::AgentModelIcon::Named(
                    ui::IconName::MavAssistant
                )),
                is_latest: false,
                disabled: None,
                cost: None,
            }]
        )])
    );
}

#[gpui::test]
async fn test_model_selection_persists_to_settings(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.create_dir(paths::settings_file().parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "default_model": {
                    "provider": "foo",
                    "model": "bar"
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    let project = Project::test(fs.clone(), [], cx).await;

    let thread_store = cx.new(|cx| ThreadStore::new(cx));

    // Create the agent and connection
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
    let connection = NativeAgentConnection(agent.clone());

    // Create a thread/session
    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/a")]),
                cx,
            )
        })
        .await
        .unwrap();

    let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

    // Select a model
    let selector = connection.model_selector(&session_id).unwrap();
    let model_id = AgentModelId::new("fake/fake");
    cx.update(|cx| selector.select_model(model_id.clone(), cx))
        .await
        .unwrap();

    // Verify the thread has the selected model
    agent.read_with(cx, |agent, _| {
        let session = agent.sessions.get(&session_id).unwrap();
        session.thread.read_with(cx, |thread, _| {
            assert_eq!(thread.model().unwrap().id().0, "fake");
        });
    });

    cx.run_until_parked();

    // Verify settings file was updated
    let settings_content = fs.load(paths::settings_file()).await.unwrap();
    let settings_json: serde_json::Value = serde_json::from_str(&settings_content).unwrap();

    // Check that the agent settings contain the selected model
    assert_eq!(
        settings_json["agent"]["default_model"]["model"],
        json!("fake")
    );
    assert_eq!(
        settings_json["agent"]["default_model"]["provider"],
        json!("fake")
    );

    // Register a thinking model and select it.
    cx.update(|cx| {
        let thinking_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "fake-corp",
            "fake-thinking",
            "Fake Thinking",
            true,
        ));
        let thinking_provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![thinking_model]),
        );
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(thinking_provider, cx);
        });
    });
    agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    // Verify enable_thinking was written to settings as true.
    let settings_content = fs.load(paths::settings_file()).await.unwrap();
    let settings_json: serde_json::Value = serde_json::from_str(&settings_content).unwrap();
    assert_eq!(
        settings_json["agent"]["default_model"]["enable_thinking"],
        json!(true),
        "selecting a thinking model should persist enable_thinking: true to settings"
    );
}

#[gpui::test]
async fn test_select_model_updates_thinking_enabled(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.create_dir(paths::settings_file().parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(paths::settings_file(), b"{}".to_vec()).await;
    let project = Project::test(fs.clone(), [], cx).await;

    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
    let connection = NativeAgentConnection(agent.clone());

    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/a")]),
                cx,
            )
        })
        .await
        .unwrap();
    let session_id = cx.update(|cx| acp_thread.read(cx).session_id().clone());

    // Register a second provider with a thinking model.
    cx.update(|cx| {
        let thinking_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
            "fake-corp",
            "fake-thinking",
            "Fake Thinking",
            true,
        ));
        let thinking_provider = Arc::new(
            FakeLanguageModelProvider::new(
                LanguageModelProviderId::from("fake-corp".to_string()),
                LanguageModelProviderName::from("Fake Corp".to_string()),
            )
            .with_models(vec![thinking_model]),
        );
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(thinking_provider, cx);
        });
    });
    // Refresh the agent's model list so it picks up the new provider.
    agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

    // Thread starts with thinking_enabled = false (the default).
    agent.read_with(cx, |agent, _| {
        let session = agent.sessions.get(&session_id).unwrap();
        session.thread.read_with(cx, |thread, _| {
            assert!(!thread.thinking_enabled(), "thinking defaults to false");
        });
    });

    // Select the thinking model via select_model.
    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
        .await
        .unwrap();

    // select_model should have enabled thinking based on the model's supports_thinking().
    agent.read_with(cx, |agent, _| {
        let session = agent.sessions.get(&session_id).unwrap();
        session.thread.read_with(cx, |thread, _| {
            assert!(
                thread.thinking_enabled(),
                "select_model should enable thinking when model supports it"
            );
        });
    });

    // Switch back to the non-thinking model.
    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake/fake"), cx))
        .await
        .unwrap();

    // select_model should have disabled thinking.
    agent.read_with(cx, |agent, _| {
        let session = agent.sessions.get(&session_id).unwrap();
        session.thread.read_with(cx, |thread, _| {
            assert!(
                !thread.thinking_enabled(),
                "select_model should disable thinking when model does not support it"
            );
        });
    });
}

#[gpui::test]
async fn test_summarization_model_survives_transient_registry_clearing(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {} })).await;
    let project = Project::test(fs.clone(), [], cx).await;

    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));

    let acp_thread = cx
        .update(|cx| {
            connection.clone().new_session(
                project.clone(),
                PathList::new(&[Path::new("/a")]),
                cx,
            )
        })
        .await
        .unwrap();
    let session_id = acp_thread.read_with(cx, |thread, _| thread.session_id().clone());

    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });

    thread.read_with(cx, |thread, _| {
        assert!(
            thread.summarization_model().is_some(),
            "session should have a summarization model from the test registry"
        );
    });

    // Simulate what happens during a provider blip:
    // update_active_language_model_from_settings calls set_default_model(None)
    // when it can't resolve the model, clearing all fallbacks.
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.set_default_model(None, cx);
        });
    });
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert!(
            thread.summarization_model().is_some(),
            "summarization model should survive a transient default model clearing"
        );
    });
}

