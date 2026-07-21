#[gpui::test]
async fn test_loaded_thread_preserves_thinking_enabled(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {} })).await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));

    // Register a thinking model.
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
        .with_models(vec![thinking_model.clone()]),
    );
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(thinking_provider, cx);
        });
    });
    agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

    // Create a thread and select the thinking model.
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

    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/fake-thinking"), cx))
        .await
        .unwrap();

    // Verify thinking is enabled after selecting the thinking model.
    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    thread.read_with(cx, |thread, _| {
        assert!(
            thread.thinking_enabled(),
            "thinking should be enabled after selecting thinking model"
        );
    });

    // Send a message so the thread gets persisted.
    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    thinking_model.send_last_completion_stream_text_chunk("Response.");
    thinking_model.end_last_completion_stream();

    send.await.unwrap();
    cx.run_until_parked();

    // Close the session so it can be reloaded from disk.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    drop(thread);
    drop(acp_thread);
    agent.read_with(cx, |agent, _| {
        assert!(agent.sessions.is_empty());
    });

    // Reload the thread and verify thinking_enabled is still true.
    let reloaded_acp_thread = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    let reloaded_thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    reloaded_thread.read_with(cx, |thread, _| {
        assert!(
            thread.thinking_enabled(),
            "thinking_enabled should be preserved when reloading a thread with a thinking model"
        );
    });

    drop(reloaded_acp_thread);
}

#[gpui::test]
async fn test_loaded_thread_preserves_model(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {} })).await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));

    // Register a model where id() != name(), like real Anthropic models
    // (e.g. id="claude-sonnet-4-5-thinking-latest", name="Claude Sonnet 4.5 Thinking").
    let model = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "fake-corp",
        "custom-model-id",
        "Custom Model Display Name",
        false,
    ));
    let provider = Arc::new(
        FakeLanguageModelProvider::new(
            LanguageModelProviderId::from("fake-corp".to_string()),
            LanguageModelProviderName::from("Fake Corp".to_string()),
        )
        .with_models(vec![model.clone()]),
    );
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(provider, cx);
        });
    });
    agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

    // Create a thread and select the model.
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

    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/custom-model-id"), cx))
        .await
        .unwrap();

    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.model().unwrap().id().0.as_ref(),
            "custom-model-id",
            "model should be set before persisting"
        );
    });

    // Send a message so the thread gets persisted.
    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();

    model.send_last_completion_stream_text_chunk("Response.");
    model.end_last_completion_stream();

    send.await.unwrap();
    cx.run_until_parked();

    // Close the session so it can be reloaded from disk.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    drop(thread);
    drop(acp_thread);
    agent.read_with(cx, |agent, _| {
        assert!(agent.sessions.is_empty());
    });

    // Reload the thread and verify the model was preserved.
    let reloaded_acp_thread = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    let reloaded_thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    reloaded_thread.read_with(cx, |thread, _| {
        let reloaded_model = thread
            .model()
            .expect("model should be present after reload");
        assert_eq!(
            reloaded_model.id().0.as_ref(),
            "custom-model-id",
            "reloaded thread should have the same model, not fall back to the default"
        );
    });

    drop(reloaded_acp_thread);
}

async fn persist_thread_with_fake_corp_model(
    cx: &mut TestAppContext,
) -> (
    Entity<NativeAgent>,
    Rc<NativeAgentConnection>,
    Entity<Project>,
    acp::SessionId,
    Arc<FakeLanguageModelProvider>,
) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/", json!({ "a": {} })).await;
    let project = Project::test(fs.clone(), [path!("/a").as_ref()], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent = cx
        .update(|cx| NativeAgent::new(thread_store.clone(), Templates::new(), fs.clone(), cx));
    let connection = Rc::new(NativeAgentConnection(agent.clone()));

    let model = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "fake-corp",
        "custom-model-id",
        "Custom Model Display Name",
        false,
    ));
    let provider = Arc::new(
        FakeLanguageModelProvider::new(
            LanguageModelProviderId::from("fake-corp".to_string()),
            LanguageModelProviderName::from("Fake Corp".to_string()),
        )
        .with_models(vec![model.clone()]),
    );
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(provider.clone(), cx);
        });
    });
    agent.update(cx, |agent, cx| agent.models.refresh_list(cx));

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

    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("fake-corp/custom-model-id"), cx))
        .await
        .unwrap();

    let send = acp_thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
    let send = cx.foreground_executor().spawn(send);
    cx.run_until_parked();
    model.send_last_completion_stream_text_chunk("Response.");
    model.end_last_completion_stream();
    send.await.unwrap();
    cx.run_until_parked();

    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .unwrap();
    drop(acp_thread);

    (agent, connection, project, session_id, provider)
}

fn unregister_fake_corp(cx: &mut TestAppContext) {
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.unregister_provider(
                LanguageModelProviderId::from("fake-corp".to_string()),
                cx,
            );
        });
    });
}

#[gpui::test]
async fn test_loaded_thread_resolves_model_when_provider_loads_late(cx: &mut TestAppContext) {
    init_test(cx);
    let (agent, _connection, project, session_id, provider) =
        persist_thread_with_fake_corp_model(cx).await;

    // Simulate a restart where the provider hasn't fetched its model list
    // yet, so the saved selection can't be resolved at load time.
    unregister_fake_corp(cx);

    let reloaded_acp_thread = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    thread.read_with(cx, |thread, _| {
        assert!(
            thread.model().is_none(),
            "should not fall back to an unrelated model"
        );
    });

    // The original selection is persisted even while unresolved, so a save
    // during the window can't overwrite the user's choice with a fallback.
    let db_thread = thread.read_with(cx, |thread, cx| thread.to_db(cx)).await;
    let saved = db_thread.model.expect("selection should be persisted");
    assert_eq!(saved.provider, "fake-corp");
    assert_eq!(saved.model, "custom-model-id");

    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(provider.clone(), cx);
        });
    });
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread
                .model()
                .expect("model should resolve once provider loads")
                .id()
                .0
                .as_ref(),
            "custom-model-id"
        );
    });

    drop(reloaded_acp_thread);
}

#[gpui::test]
async fn test_explicit_model_selection_cancels_pending(cx: &mut TestAppContext) {
    init_test(cx);
    let (agent, connection, project, session_id, provider) =
        persist_thread_with_fake_corp_model(cx).await;

    unregister_fake_corp(cx);

    let reloaded_acp_thread = agent
        .update(cx, |agent, cx| {
            agent.open_thread(session_id.clone(), project.clone(), cx)
        })
        .await
        .unwrap();
    let thread = agent.read_with(cx, |agent, _| {
        agent.sessions.get(&session_id).unwrap().thread.clone()
    });
    thread.read_with(cx, |thread, _| {
        assert!(thread.model().is_none());
    });

    // The user explicitly picks a different, available model.
    let other_model = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "other-corp",
        "other-model-id",
        "Other Model",
        false,
    ));
    let other_provider = Arc::new(
        FakeLanguageModelProvider::new(
            LanguageModelProviderId::from("other-corp".to_string()),
            LanguageModelProviderName::from("Other Corp".to_string()),
        )
        .with_models(vec![other_model.clone()]),
    );
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(other_provider, cx);
        });
    });
    cx.run_until_parked();

    let selector = connection.model_selector(&session_id).unwrap();
    cx.update(|cx| selector.select_model(AgentModelId::new("other-corp/other-model-id"), cx))
        .await
        .unwrap();

    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.model().unwrap().id().0.as_ref(), "other-model-id");
    });

    // The original provider returning must not clobber the explicit choice.
    cx.update(|cx| {
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry.register_provider(provider.clone(), cx);
        });
    });
    cx.run_until_parked();

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.model().unwrap().id().0.as_ref(),
            "other-model-id",
            "a late provider load must not override the explicit selection"
        );
    });

    drop(reloaded_acp_thread);
}

