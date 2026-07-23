use super::*;
use gpui::TestAppContext;
use http_client::FakeHttpClient;
use parking_lot::Mutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

struct FakeCredentialsProvider {
    storage: Mutex<Option<(String, Vec<u8>)>>,
}

impl FakeCredentialsProvider {
    fn new() -> Self {
        Self {
            storage: Mutex::new(None),
        }
    }
}

impl CredentialsProvider for FakeCredentialsProvider {
    fn read_credentials<'a>(
        &'a self,
        _url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>> {
        Box::pin(async { Ok(self.storage.lock().clone()) })
    }

    fn write_credentials<'a>(
        &'a self,
        _url: &'a str,
        username: &'a str,
        password: &'a [u8],
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        self.storage
            .lock()
            .replace((username.to_string(), password.to_vec()));
        Box::pin(async { Ok(()) })
    }

    fn delete_credentials<'a>(
        &'a self,
        _url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        *self.storage.lock() = None;
        Box::pin(async { Ok(()) })
    }
}

fn make_expired_credentials() -> CodexCredentials {
    CodexCredentials {
        access_token: "old_access".to_string(),
        refresh_token: "old_refresh".to_string(),
        expires_at_ms: 0,
        account_id: None,
        email: None,
    }
}

fn make_fresh_credentials() -> CodexCredentials {
    CodexCredentials {
        access_token: "fresh_access".to_string(),
        refresh_token: "fresh_refresh".to_string(),
        expires_at_ms: now_ms() + 3_600_000,
        account_id: None,
        email: None,
    }
}

fn fake_token_response() -> String {
    serde_json::json!({
        "access_token": "fresh_access",
        "refresh_token": "fresh_refresh",
        "expires_in": 3600
    })
    .to_string()
}

#[gpui::test]
async fn test_concurrent_refresh_deduplicates(cx: &mut TestAppContext) {
    let refresh_count = Arc::new(AtomicUsize::new(0));
    let refresh_count_clone = refresh_count.clone();

    let http_client = FakeHttpClient::create(move |_request| {
        let refresh_count = refresh_count_clone.clone();
        async move {
            refresh_count.fetch_add(1, Ordering::SeqCst);
            let body = fake_token_response();
            Ok(http_client::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(body))?)
        }
    });

    let state = cx.new(|_cx| State {
        credentials: Some(make_expired_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: Arc::new(FakeCredentialsProvider::new()),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    // Spawn two concurrent refresh attempts.
    let weak1 = weak_state.clone();
    let http1 = http.clone();
    let task1 = cx.spawn(async move |mut cx| get_fresh_credentials(&weak1, &http1, &mut cx).await);

    let weak2 = weak_state.clone();
    let http2 = http.clone();
    let task2 = cx.spawn(async move |mut cx| get_fresh_credentials(&weak2, &http2, &mut cx).await);

    // Drive both to completion.
    cx.run_until_parked();
    let result1 = task1.await;
    let result2 = task2.await;

    assert!(result1.is_ok(), "first refresh should succeed");
    assert!(result2.is_ok(), "second refresh should succeed");
    assert_eq!(result1.unwrap().access_token, "fresh_access");
    assert_eq!(result2.unwrap().access_token, "fresh_access");
    assert_eq!(
        refresh_count.load(Ordering::SeqCst),
        1,
        "refresh_token should only be called once despite two concurrent callers"
    );
}

#[gpui::test]
async fn test_fresh_credentials_skip_refresh(cx: &mut TestAppContext) {
    let refresh_count = Arc::new(AtomicUsize::new(0));
    let refresh_count_clone = refresh_count.clone();

    let http_client = FakeHttpClient::create(move |_request| {
        let refresh_count = refresh_count_clone.clone();
        async move {
            refresh_count.fetch_add(1, Ordering::SeqCst);
            let body = fake_token_response();
            Ok(http_client::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(body))?)
        }
    });

    let state = cx.new(|_cx| State {
        credentials: Some(make_fresh_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: Arc::new(FakeCredentialsProvider::new()),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    let weak = weak_state.clone();
    let http_clone = http.clone();
    let result = cx
        .spawn(async move |mut cx| get_fresh_credentials(&weak, &http_clone, &mut cx).await)
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().access_token, "fresh_access");
    assert_eq!(
        refresh_count.load(Ordering::SeqCst),
        0,
        "no refresh should happen when credentials are fresh"
    );
}

#[gpui::test]
async fn test_no_credentials_returns_no_api_key(cx: &mut TestAppContext) {
    let http_client = FakeHttpClient::create(|_| async {
        Ok(http_client::Response::builder()
            .status(200)
            .body(http_client::AsyncBody::default())?)
    });

    let state = cx.new(|_cx| State {
        credentials: None,
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: Arc::new(FakeCredentialsProvider::new()),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    let weak = weak_state.clone();
    let http_clone = http.clone();
    let result = cx
        .spawn(async move |mut cx| get_fresh_credentials(&weak, &http_clone, &mut cx).await)
        .await;

    assert!(matches!(
        result,
        Err(LanguageModelCompletionError::NoApiKey { .. })
    ));
}

#[gpui::test]
async fn test_fatal_refresh_clears_auth_state(cx: &mut TestAppContext) {
    let http_client = FakeHttpClient::create(move |_request| async move {
        Ok(http_client::Response::builder()
            .status(401)
            .body(http_client::AsyncBody::from(r#"{"error":"invalid_grant"}"#))?)
    });

    let creds_provider = Arc::new(FakeCredentialsProvider::new());
    let state = cx.new(|_cx| State {
        credentials: Some(make_expired_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: creds_provider.clone(),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    let weak = weak_state.clone();
    let http_clone = http.clone();
    let result = cx
        .spawn(async move |mut cx| get_fresh_credentials(&weak, &http_clone, &mut cx).await)
        .await;

    cx.run_until_parked();

    assert!(result.is_err(), "fatal refresh should return an error");
    cx.read(|cx| {
        let s = state.read(cx);
        assert!(
            s.credentials.is_none(),
            "credentials should be cleared on fatal refresh failure"
        );
        assert!(
            s.last_auth_error.is_some(),
            "last_auth_error should be set on fatal refresh failure"
        );
    });
}

#[gpui::test]
async fn test_transient_refresh_keeps_credentials(cx: &mut TestAppContext) {
    let http_client = FakeHttpClient::create(move |_request| async move {
        Ok(http_client::Response::builder()
            .status(500)
            .body(http_client::AsyncBody::from("Internal Server Error"))?)
    });

    let state = cx.new(|_cx| State {
        credentials: Some(make_expired_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: Arc::new(FakeCredentialsProvider::new()),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    let weak = weak_state.clone();
    let http_clone = http.clone();
    let result = cx
        .spawn(async move |mut cx| get_fresh_credentials(&weak, &http_clone, &mut cx).await)
        .await;

    cx.run_until_parked();

    assert!(result.is_err(), "transient refresh should return an error");
    cx.read(|cx| {
        let s = state.read(cx);
        assert!(
            s.credentials.is_some(),
            "credentials should be kept on transient refresh failure"
        );
        assert!(
            s.last_auth_error.is_none(),
            "last_auth_error should not be set on transient refresh failure"
        );
    });
}

#[gpui::test]
async fn test_sign_out_during_refresh_discards_result(cx: &mut TestAppContext) {
    let (gate_tx, gate_rx) = futures::channel::oneshot::channel::<()>();
    let gate_rx = Arc::new(Mutex::new(Some(gate_rx)));
    let gate_rx_clone = gate_rx.clone();

    let http_client = FakeHttpClient::create(move |_request| {
        let gate_rx = gate_rx_clone.clone();
        async move {
            // Wait until the gate is opened, simulating a slow network.
            let rx = gate_rx.lock().take();
            if let Some(rx) = rx {
                let _ = rx.await;
            }
            let body = fake_token_response();
            Ok(http_client::Response::builder()
                .status(200)
                .body(http_client::AsyncBody::from(body))?)
        }
    });

    let creds_provider = Arc::new(FakeCredentialsProvider::new());
    let state = cx.new(|_cx| State {
        credentials: Some(make_expired_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: creds_provider.clone(),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let http: Arc<dyn HttpClient> = http_client;

    // Start a refresh
    let weak = weak_state.clone();
    let http_clone = http.clone();
    let refresh_task =
        cx.spawn(async move |mut cx| get_fresh_credentials(&weak, &http_clone, &mut cx).await);

    cx.run_until_parked();

    // Sign out while the refresh is in-flight
    cx.update(|cx| {
        do_sign_out(&weak_state, cx).detach();
    });
    cx.run_until_parked();

    // Now let the refresh respond by opening the gate
    let _ = gate_tx.send(());
    cx.run_until_parked();

    let result = refresh_task.await;
    assert!(result.is_err(), "refresh should fail after sign-out");

    cx.read(|cx| {
        let s = state.read(cx);
        assert!(
            s.credentials.is_none(),
            "sign-out should have cleared credentials"
        );
    });
}

#[gpui::test]
async fn test_sign_out_completes_fully(cx: &mut TestAppContext) {
    let creds_provider = Arc::new(FakeCredentialsProvider::new());
    // Pre-populate the credential store
    creds_provider
        .storage
        .lock()
        .replace(("Bearer".to_string(), b"some-creds".to_vec()));

    let state = cx.new(|_cx| State {
        credentials: Some(make_fresh_credentials()),
        sign_in_task: None,
        refresh_task: None,
        load_task: None,
        credentials_provider: creds_provider.clone(),
        auth_generation: 0,
        last_auth_error: None,
    });

    let weak_state = cx.read(|_cx| state.downgrade());
    let sign_out_task = cx.update(|cx| do_sign_out(&weak_state, cx));

    cx.run_until_parked();
    sign_out_task.await.expect("sign-out should succeed");

    assert!(
        creds_provider.storage.lock().is_none(),
        "credential store should be empty after sign-out"
    );
    cx.read(|cx| {
        assert!(
            !state.read(cx).is_authenticated(),
            "state should show not authenticated"
        );
    });
}

#[gpui::test]
async fn test_authenticate_awaits_initial_load(cx: &mut TestAppContext) {
    let creds = make_fresh_credentials();
    let creds_json = serde_json::to_vec(&creds).unwrap();
    let creds_provider = Arc::new(FakeCredentialsProvider::new());
    creds_provider
        .storage
        .lock()
        .replace(("Bearer".to_string(), creds_json));

    let http_client = FakeHttpClient::create(|_| async {
        Ok(http_client::Response::builder()
            .status(200)
            .body(http_client::AsyncBody::default())?)
    });

    let provider = cx.update(|cx| OpenAiSubscribedProvider::new(http_client, creds_provider, cx));

    // Before load completes, authenticate should still await the load.
    let auth_task = cx.update(|cx| provider.authenticate(cx));

    // Drive the load to completion.
    cx.run_until_parked();

    let result = auth_task.await;
    assert!(
        result.is_ok(),
        "authenticate should succeed after load completes with valid credentials"
    );
}
