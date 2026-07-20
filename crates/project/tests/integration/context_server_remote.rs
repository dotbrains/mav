use crate::context_server_store::*;

#[gpui::test]
async fn test_remote_context_server(cx: &mut TestAppContext) {
    const SERVER_ID: &str = "remote-server";
    let server_id = ContextServerId(SERVER_ID.into());
    let server_url = "http://example.com/api";

    let client = FakeHttpClient::create(|_| async move {
        use http_client::AsyncBody;

        let response = Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(AsyncBody::from(
                serde_json::to_string(&json!({
                    "jsonrpc": "2.0",
                    "id": 0,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "serverInfo": {
                            "name": "test-server",
                            "version": "1.0.0"
                        }
                    }
                }))
                .unwrap(),
            ))
            .unwrap();
        Ok(response)
    });
    cx.update(|cx| cx.set_http_client(client));

    let (_fs, project) = setup_context_server_test(cx, json!({ "code.rs": "" }), vec![]).await;

    let store = project.read_with(cx, |project, _| project.context_server_store());

    set_context_server_configuration(
        vec![(
            server_id.0.clone(),
            settings::ContextServerSettingsContent::Http {
                enabled: true,
                url: server_url.to_string(),
                headers: Default::default(),
                timeout: None,
                oauth: None,
            },
        )],
        cx,
    );

    let _server_events = assert_server_events(
        &store,
        vec![
            (server_id.clone(), ContextServerStatus::Starting),
            (server_id.clone(), ContextServerStatus::Running),
        ],
        cx,
    );
    cx.run_until_parked();
}
