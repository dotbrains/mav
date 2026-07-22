use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{FakeServer, parse_authorization_header};

    use clock::FakeSystemClock;
    use gpui::{AppContext as _, BackgroundExecutor, TestAppContext};
    use http_client::FakeHttpClient;
    use parking_lot::Mutex;
    use proto::TypedEnvelope;
    use settings::SettingsStore;
    use std::future;

    #[test]
    fn test_proxy_settings_trims_and_ignores_empty_proxy() {
        let mut content = SettingsContent::default();
        content.proxy = Some("   ".to_owned());
        assert_eq!(ProxySettings::from_settings(&content).proxy, None);

        content.proxy = Some("http://127.0.0.1:10809".to_owned());
        assert_eq!(
            ProxySettings::from_settings(&content).proxy.as_deref(),
            Some("http://127.0.0.1:10809")
        );
    }

    #[gpui::test(iterations = 10)]
    async fn test_reconnection(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;
        let mut status = client.status();
        assert!(matches!(
            status.next().await,
            Some(Status::Connected { .. })
        ));
        assert_eq!(server.auth_count(), 1);

        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        server.allow_connections();
        cx.executor().advance_clock(Duration::from_secs(10));
        while !matches!(status.next().await, Some(Status::Connected { .. })) {}
        assert_eq!(server.auth_count(), 1); // Client reused the cached credentials when reconnecting

        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        // Clear cached credentials after authentication fails
        server.roll_access_token();
        server.allow_connections();
        cx.executor().run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(10));
        while !matches!(status.next().await, Some(Status::Connected { .. })) {}
        assert_eq!(server.auth_count(), 2); // Client re-authenticated due to an invalid token
    }

    #[gpui::test(iterations = 10)]
    async fn test_auth_failure_during_reconnection(cx: &mut TestAppContext) {
        init_test(cx);
        let http_client = FakeHttpClient::with_200_response();
        let client =
            cx.update(|cx| Client::new(Arc::new(FakeSystemClock::new()), http_client.clone(), cx));
        let server = FakeServer::for_client(42, &client, cx).await;
        let mut status = client.status();
        assert!(matches!(
            status.next().await,
            Some(Status::Connected { .. })
        ));
        assert_eq!(server.auth_count(), 1);

        // Simulate an auth failure during reconnection.
        http_client
            .as_fake()
            .replace_handler(|_, _request| async move {
                Ok(http_client::Response::builder()
                    .status(503)
                    .body("".into())
                    .unwrap())
            });
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        // Restore the ability to authenticate.
        http_client
            .as_fake()
            .replace_handler(|_, _request| async move {
                Ok(http_client::Response::builder()
                    .status(200)
                    .body("".into())
                    .unwrap())
            });
        cx.executor().advance_clock(Duration::from_secs(10));
        while !matches!(status.next().await, Some(Status::Connected { .. })) {}
        assert_eq!(server.auth_count(), 1); // Client reused the cached credentials when reconnecting
    }

    #[gpui::test(iterations = 10)]
    async fn test_connection_timeout(executor: BackgroundExecutor, cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let mut status = client.status();

        // Time out when client tries to connect.
        client.override_authenticate(move |cx| {
            cx.background_spawn(async move {
                Ok(Credentials {
                    user_id,
                    access_token: "token".into(),
                })
            })
        });
        client.override_establish_connection(|_, cx| {
            cx.background_spawn(async move {
                future::pending::<()>().await;
                unreachable!()
            })
        });
        let auth_and_connect = cx.spawn({
            let client = client.clone();
            |cx| async move { client.connect(false, &cx).await }
        });
        executor.run_until_parked();
        assert!(matches!(status.next().await, Some(Status::Connecting)));

        executor.advance_clock(CONNECTION_TIMEOUT);
        assert!(matches!(status.next().await, Some(Status::ConnectionError)));
        auth_and_connect.await.into_response().unwrap_err();

        // Allow the connection to be established.
        let server = FakeServer::for_client(user_id, &client, cx).await;
        assert!(matches!(
            status.next().await,
            Some(Status::Connected { .. })
        ));

        // Disconnect client.
        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        // Time out when re-establishing the connection.
        server.allow_connections();
        client.override_establish_connection(|_, cx| {
            cx.background_spawn(async move {
                future::pending::<()>().await;
                unreachable!()
            })
        });
        executor.advance_clock(2 * INITIAL_RECONNECTION_DELAY);
        assert!(matches!(status.next().await, Some(Status::Reconnecting)));

        executor.advance_clock(CONNECTION_TIMEOUT);
        assert!(matches!(
            status.next().await,
            Some(Status::ReconnectionError { .. })
        ));
    }

    #[gpui::test(iterations = 10)]
    async fn test_reauthenticate_only_if_unauthorized(cx: &mut TestAppContext) {
        init_test(cx);
        let auth_count = Arc::new(Mutex::new(0));
        let http_client = FakeHttpClient::create(|_request| async move {
            Ok(http_client::Response::builder()
                .status(200)
                .body("".into())
                .unwrap())
        });
        let client =
            cx.update(|cx| Client::new(Arc::new(FakeSystemClock::new()), http_client.clone(), cx));
        client.override_authenticate({
            let auth_count = auth_count.clone();
            move |cx| {
                let auth_count = auth_count.clone();
                cx.background_spawn(async move {
                    *auth_count.lock() += 1;
                    Ok(Credentials {
                        user_id: 1,
                        access_token: auth_count.lock().to_string(),
                    })
                })
            }
        });

        let credentials = client.sign_in(false, &cx.to_async()).await.unwrap();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(credentials.access_token, "1");

        // If credentials are still valid, signing in doesn't trigger authentication.
        let credentials = client.sign_in(false, &cx.to_async()).await.unwrap();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(credentials.access_token, "1");

        // If the server is unavailable, signing in doesn't trigger authentication.
        http_client
            .as_fake()
            .replace_handler(|_, _request| async move {
                Ok(http_client::Response::builder()
                    .status(503)
                    .body("".into())
                    .unwrap())
            });
        client.sign_in(false, &cx.to_async()).await.unwrap_err();
        assert_eq!(*auth_count.lock(), 1);

        // If credentials became invalid, signing in triggers authentication.
        http_client
            .as_fake()
            .replace_handler(|_, request| async move {
                let credentials = parse_authorization_header(&request).unwrap();
                if credentials.access_token == "2" {
                    Ok(http_client::Response::builder()
                        .status(200)
                        .body("".into())
                        .unwrap())
                } else {
                    Ok(http_client::Response::builder()
                        .status(401)
                        .body("".into())
                        .unwrap())
                }
            });
        let credentials = client.sign_in(false, &cx.to_async()).await.unwrap();
        assert_eq!(*auth_count.lock(), 2);
        assert_eq!(credentials.access_token, "2");
    }

    #[gpui::test]
    async fn test_sign_in_reports_connection_failure(cx: &mut TestAppContext) {
        init_test(cx);
        let http_client = FakeHttpClient::create(|_request| async move {
            Ok(http_client::Response::builder()
                .status(200)
                .body("".into())
                .unwrap())
        });
        let client =
            cx.update(|cx| Client::new(Arc::new(FakeSystemClock::new()), http_client.clone(), cx));
        client.override_authenticate(move |cx| {
            cx.background_spawn(async move {
                Ok(Credentials {
                    user_id: 1,
                    access_token: "token".into(),
                })
            })
        });

        // Sign in once so that the credentials are cached on the client.
        client.sign_in(false, &cx.to_async()).await.unwrap();

        // Simulate a transport-level failure (DNS/TCP/TLS/timeout) where the
        // request never receives a response while validating cached credentials.
        http_client
            .as_fake()
            .replace_handler(|_, _request| async move {
                Err(anyhow!("connection reset by peer").context("boom"))
            });

        let error = client.sign_in(false, &cx.to_async()).await.unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "failed to validate credentials: boom: connection reset by peer"
        );
    }

    #[gpui::test(iterations = 10)]
    async fn test_authenticating_more_than_once(
        cx: &mut TestAppContext,
        executor: BackgroundExecutor,
    ) {
        init_test(cx);
        let auth_count = Arc::new(Mutex::new(0));
        let dropped_auth_count = Arc::new(Mutex::new(0));
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        client.override_authenticate({
            let auth_count = auth_count.clone();
            let dropped_auth_count = dropped_auth_count.clone();
            move |cx| {
                let auth_count = auth_count.clone();
                let dropped_auth_count = dropped_auth_count.clone();
                cx.background_spawn(async move {
                    *auth_count.lock() += 1;
                    let _drop = util::defer(move || *dropped_auth_count.lock() += 1);
                    future::pending::<()>().await;
                    unreachable!()
                })
            }
        });

        let _authenticate = cx.spawn({
            let client = client.clone();
            move |cx| async move { client.connect(false, &cx).await }
        });
        executor.run_until_parked();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(*dropped_auth_count.lock(), 0);

        let _authenticate = cx.spawn(|cx| async move { client.connect(false, &cx).await });
        executor.run_until_parked();
        assert_eq!(*auth_count.lock(), 2);
        assert_eq!(*dropped_auth_count.lock(), 1);
    }

    #[gpui::test]
    async fn test_subscribing_to_entity(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let (done_tx1, done_rx1) = async_channel::unbounded();
        let (done_tx2, done_rx2) = async_channel::unbounded();
        AnyProtoClient::from(client.clone()).add_entity_message_handler(
            move |entity: Entity<TestEntity>, _: TypedEnvelope<proto::JoinProject>, cx| {
                match entity.read_with(&cx, |entity, _| entity.id) {
                    1 => done_tx1.try_send(()).unwrap(),
                    2 => done_tx2.try_send(()).unwrap(),
                    _ => unreachable!(),
                }
                async { Ok(()) }
            },
        );
        let entity1 = cx.new(|_| TestEntity {
            id: 1,
            subscription: None,
        });
        let entity2 = cx.new(|_| TestEntity {
            id: 2,
            subscription: None,
        });
        let entity3 = cx.new(|_| TestEntity {
            id: 3,
            subscription: None,
        });

        let _subscription1 = client
            .subscribe_to_entity(1)
            .unwrap()
            .set_entity(&entity1, &cx.to_async());
        let _subscription2 = client
            .subscribe_to_entity(2)
            .unwrap()
            .set_entity(&entity2, &cx.to_async());
        // Ensure dropping a subscription for the same entity type still allows receiving of
        // messages for other entity IDs of the same type.
        let subscription3 = client
            .subscribe_to_entity(3)
            .unwrap()
            .set_entity(&entity3, &cx.to_async());
        drop(subscription3);

        server.send(proto::JoinProject {
            project_id: 1,
            committer_name: None,
            committer_email: None,
            features: Vec::new(),
        });
        server.send(proto::JoinProject {
            project_id: 2,
            committer_name: None,
            committer_email: None,
            features: Vec::new(),
        });
        done_rx1.recv().await.unwrap();
        done_rx2.recv().await.unwrap();
    }

    #[gpui::test]
    async fn test_subscribing_after_dropping_subscription(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let entity = cx.new(|_| TestEntity::default());
        let (done_tx1, _done_rx1) = async_channel::unbounded();
        let (done_tx2, done_rx2) = async_channel::unbounded();
        let subscription1 = client.add_message_handler(
            entity.downgrade(),
            move |_, _: TypedEnvelope<proto::Ping>, _| {
                done_tx1.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        drop(subscription1);
        let _subscription2 = client.add_message_handler(
            entity.downgrade(),
            move |_, _: TypedEnvelope<proto::Ping>, _| {
                done_tx2.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        server.send(proto::Ping {});
        done_rx2.recv().await.unwrap();
    }

    #[gpui::test]
    async fn test_dropping_subscription_in_handler(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let entity = cx.new(|_| TestEntity::default());
        let (done_tx, done_rx) = async_channel::unbounded();
        let subscription = client.add_message_handler(
            entity.clone().downgrade(),
            move |entity: Entity<TestEntity>, _: TypedEnvelope<proto::Ping>, mut cx| {
                entity
                    .update(&mut cx, |entity, _| entity.subscription.take())
                    .unwrap();
                done_tx.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        entity.update(cx, |entity, _| {
            entity.subscription = Some(subscription);
        });
        server.send(proto::Ping {});
        done_rx.recv().await.unwrap();
    }

    #[derive(Default)]
    struct TestEntity {
        id: usize,
        subscription: Option<Subscription>,
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }
}
