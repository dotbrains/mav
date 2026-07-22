use super::*;
use gpui::TestAppContext;
use std::str::FromStr;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test]
async fn test_fake(cx: &mut TestAppContext) {
    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    let (server, mut fake) = FakeLanguageServer::new(
        LanguageServerId(0),
        LanguageServerBinary {
            path: "path/to/language-server".into(),
            arguments: vec![],
            env: None,
        },
        "the-lsp".to_string(),
        Default::default(),
        &mut cx.to_async(),
    );

    let (message_tx, message_rx) = channel::unbounded();
    let (diagnostics_tx, diagnostics_rx) = channel::unbounded();
    server
        .on_notification::<notification::ShowMessage, _>(move |params, _| {
            message_tx.try_send(params).unwrap()
        })
        .detach();
    server
        .on_notification::<notification::PublishDiagnostics, _>(move |params, _| {
            diagnostics_tx.try_send(params).unwrap()
        })
        .detach();

    let server = cx
        .update(|cx| {
            let params = server.default_initialize_params(false, false, cx);
            let configuration = DidChangeConfigurationParams {
                settings: Default::default(),
            };
            server.initialize(
                params,
                configuration.into(),
                DEFAULT_LSP_REQUEST_TIMEOUT,
                cx,
            )
        })
        .await
        .unwrap();
    server
        .notify::<notification::DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(
                Uri::from_str("file://a/b").unwrap(),
                "rust".to_string(),
                0,
                "".to_string(),
            ),
        })
        .unwrap();
    assert_eq!(
        fake.receive_notification::<notification::DidOpenTextDocument>()
            .await
            .text_document
            .uri
            .as_str(),
        "file://a/b"
    );

    fake.notify::<notification::ShowMessage>(ShowMessageParams {
        typ: MessageType::ERROR,
        message: "ok".to_string(),
    });
    fake.notify::<notification::PublishDiagnostics>(PublishDiagnosticsParams {
        uri: Uri::from_str("file://b/c").unwrap(),
        version: Some(5),
        diagnostics: vec![],
    });
    assert_eq!(message_rx.recv().await.unwrap().message, "ok");
    assert_eq!(
        diagnostics_rx.recv().await.unwrap().uri.as_str(),
        "file://b/c"
    );

    fake.set_request_handler::<request::Shutdown, _, _>(|_, _| async move { Ok(()) });

    drop(server);
    cx.run_until_parked();
    fake.receive_notification::<notification::Exit>().await;
}

#[gpui::test]
async fn test_subscription_leaks_handlers_after_server_drop(cx: &mut TestAppContext) {
    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    let (server, mut fake) = FakeLanguageServer::new(
        LanguageServerId(0),
        LanguageServerBinary {
            path: "path/to/language-server".into(),
            arguments: vec![],
            env: None,
        },
        "the-lsp".to_string(),
        Default::default(),
        &mut cx.to_async(),
    );

    let detached_payload = Arc::new(());
    let detached_payload_handle = Arc::downgrade(&detached_payload);
    server
        .on_notification::<notification::ShowMessage, _>(move |_, _| {
            let _payload = &detached_payload;
        })
        .detach();

    let retained_payload = Arc::new(());
    let retained_payload_handle = Arc::downgrade(&retained_payload);
    let subscription =
        server.on_notification::<notification::PublishDiagnostics, _>(move |_, _| {
            let _payload = &retained_payload;
        });

    let server = cx
        .update(|cx| {
            let params = server.default_initialize_params(false, false, cx);
            let configuration = DidChangeConfigurationParams {
                settings: Default::default(),
            };
            server.initialize(
                params,
                configuration.into(),
                DEFAULT_LSP_REQUEST_TIMEOUT,
                cx,
            )
        })
        .await
        .unwrap();

    drop(server);
    cx.run_until_parked();
    fake.receive_notification::<notification::Exit>().await;
    drop(fake);
    cx.run_until_parked();

    assert!(
        detached_payload_handle.upgrade().is_none(),
        "detached handler was kept alive after the server was dropped, \
            because an unrelated retained subscription pins the whole handler map"
    );
    assert!(
        retained_payload_handle.upgrade().is_none(),
        "handler with a retained subscription was kept alive after the server was dropped"
    );

    drop(subscription);
    assert!(detached_payload_handle.upgrade().is_none());
    assert!(retained_payload_handle.upgrade().is_none());
}

#[gpui::test]
fn test_deserialize_string_digit_id() {
    let json = r#"{"jsonrpc":"2.0","id":"2","method":"workspace/configuration","params":{"items":[{"scopeUri":"file:///Users/mph/Devel/personal/hello-scala/","section":"metals"}]}}"#;
    let notification = serde_json::from_str::<NotificationOrRequest>(json)
        .expect("message with string id should be parsed");
    let expected_id = RequestId::Str("2".to_string());
    assert_eq!(notification.id, Some(expected_id));
}

#[gpui::test]
fn test_deserialize_string_id() {
    let json = r#"{"jsonrpc":"2.0","id":"anythingAtAll","method":"workspace/configuration","params":{"items":[{"scopeUri":"file:///Users/mph/Devel/personal/hello-scala/","section":"metals"}]}}"#;
    let notification = serde_json::from_str::<NotificationOrRequest>(json)
        .expect("message with string id should be parsed");
    let expected_id = RequestId::Str("anythingAtAll".to_string());
    assert_eq!(notification.id, Some(expected_id));
}

#[gpui::test]
fn test_deserialize_int_id() {
    let json = r#"{"jsonrpc":"2.0","id":2,"method":"workspace/configuration","params":{"items":[{"scopeUri":"file:///Users/mph/Devel/personal/hello-scala/","section":"metals"}]}}"#;
    let notification = serde_json::from_str::<NotificationOrRequest>(json)
        .expect("message with string id should be parsed");
    let expected_id = RequestId::Int(2);
    assert_eq!(notification.id, Some(expected_id));
}

#[test]
fn test_serialize_has_no_nulls() {
    // Ensure we're not setting both result and error variants. (ticket #10595)
    let no_tag = Response::<u32> {
        jsonrpc: "",
        id: RequestId::Int(0),
        value: LspResult::Ok(None),
    };
    assert_eq!(
        serde_json::to_string(&no_tag).unwrap(),
        "{\"jsonrpc\":\"\",\"id\":0,\"result\":null}"
    );
    let no_tag = Response::<u32> {
        jsonrpc: "",
        id: RequestId::Int(0),
        value: LspResult::Error(None),
    };
    assert_eq!(
        serde_json::to_string(&no_tag).unwrap(),
        "{\"jsonrpc\":\"\",\"id\":0,\"error\":null}"
    );
}

#[gpui::test]
async fn test_initialize_params_has_root_path_and_root_uri(cx: &mut TestAppContext) {
    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    let (server, _fake) = FakeLanguageServer::new(
        LanguageServerId(0),
        LanguageServerBinary {
            path: "path/to/language-server".into(),
            arguments: vec![],
            env: None,
        },
        "test-lsp".to_string(),
        Default::default(),
        &mut cx.to_async(),
    );

    let params = cx.update(|cx| server.default_initialize_params(false, false, cx));

    #[allow(deprecated)]
    let root_uri = params.root_uri.expect("root_uri should be set");
    #[allow(deprecated)]
    let root_path = params.root_path.expect("root_path should be set");

    let expected_path = root_uri
        .to_file_path()
        .expect("root_uri should be a valid file path");
    assert_eq!(
        root_path,
        expected_path.to_string_lossy(),
        "root_path should be derived from root_uri"
    );
}
