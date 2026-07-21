use super::tests::*;
use super::*;

#[gpui::test]
async fn test_auth_required_on_initial_connect(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = AuthGatedAgentConnection::new();
    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;

    conversation_view.read_with(cx, |view, _cx| {
        let connected = view
            .as_connected()
            .expect("Should be in Connected state even though auth is required");
        assert!(
            !connected.auth_state.is_ok(),
            "Auth state should be Unauthenticated"
        );
        assert!(
            !view.supports_logout(),
            "Logout should be hidden while unauthenticated"
        );
        assert!(
            connected.active_id.is_none(),
            "There should be no active thread since no session was created"
        );
        assert!(
            connected.threads.is_empty(),
            "There should be no threads since no session was created"
        );
    });

    conversation_view.read_with(cx, |view, _cx| {
        assert!(
            view.active_thread().is_none(),
            "active_thread() should be None when unauthenticated without a session"
        );
    });

    conversation_view.update_in(cx, |view, window, cx| {
        view.authenticate(
            acp::AuthMethodId::new(AuthGatedAgentConnection::AUTH_METHOD_ID),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        let connected = view
            .as_connected()
            .expect("Should still be in Connected state after auth");
        assert!(connected.auth_state.is_ok(), "Auth state should be Ok");
        assert!(
            view.supports_logout(),
            "Logout should be available after authentication"
        );
        assert!(
            connected.active_id.is_some(),
            "There should be an active thread after successful auth"
        );
        assert_eq!(
            connected.threads.len(),
            1,
            "There should be exactly one thread"
        );

        let active = view
            .active_thread()
            .expect("active_thread() should return the new thread");
        assert!(
            active.read(cx).thread_error.is_none(),
            "The new thread should have no errors"
        );
    });

    conversation_view.update_in(cx, |view, window, cx| view.logout(window, cx));
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, _cx| {
        let connected = view
            .as_connected()
            .expect("Should still be in Connected state after logout");
        assert!(
            !connected.auth_state.is_ok(),
            "Auth state should be Unauthenticated after logout"
        );
        assert!(
            !view.supports_logout(),
            "Logout should be hidden after logout"
        );
    });
}
