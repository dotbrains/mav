use crate::context_server_store::*;

#[gpui::test]
async fn test_context_server_global_timeout(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(r#"{"context_server_timeout": 90}"#, cx)
                .expect("Failed to set test user settings");
        });
    });

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let mut async_cx = cx.to_async();
    let result = ContextServerStore::create_context_server(
        store.downgrade(),
        ContextServerId("test-server".into()),
        Arc::new(ContextServerConfiguration::Http {
            url: url::Url::parse("http://localhost:8080").expect("Failed to parse test URL"),
            headers: Default::default(),
            timeout: None,
            oauth: None,
        }),
        &mut async_cx,
    )
    .await;

    assert!(
        result.is_ok(),
        "Server should be created successfully with global timeout"
    );
}

#[gpui::test]
async fn test_context_server_per_server_timeout_override(cx: &mut TestAppContext) {
    const SERVER_ID: &str = "test-server";

    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        SettingsStore::update_global(cx, |store, cx| {
            store
                .set_user_settings(r#"{"context_server_timeout": 60}"#, cx)
                .expect("Failed to set test user settings");
        });
    });

    let (_fs, project) = setup_context_server_test(
        cx,
        json!({"code.rs": ""}),
        vec![(
            SERVER_ID.into(),
            ContextServerSettings::Http {
                enabled: true,
                url: "http://localhost:8080".to_string(),
                headers: Default::default(),
                timeout: Some(120),
                oauth: None,
            },
        )],
    )
    .await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let mut async_cx = cx.to_async();
    let result = ContextServerStore::create_context_server(
        store.downgrade(),
        ContextServerId("test-server".into()),
        Arc::new(ContextServerConfiguration::Http {
            url: url::Url::parse("http://localhost:8080").expect("Failed to parse test URL"),
            headers: Default::default(),
            timeout: Some(120),
            oauth: None,
        }),
        &mut async_cx,
    )
    .await;

    assert!(
        result.is_ok(),
        "Server should be created successfully with per-server timeout override"
    );
}

#[gpui::test]
async fn test_context_server_stdio_timeout(cx: &mut TestAppContext) {
    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let mut async_cx = cx.to_async();
    let result = ContextServerStore::create_context_server(
        store.downgrade(),
        ContextServerId("stdio-server".into()),
        Arc::new(ContextServerConfiguration::Custom {
            command: ContextServerCommand {
                path: "/usr/bin/node".into(),
                args: vec!["server.js".into()],
                env: None,
                timeout: Some(180000),
            },
            remote: false,
        }),
        &mut async_cx,
    )
    .await;

    assert!(
        result.is_ok(),
        "Stdio server should be created successfully with timeout"
    );
}
