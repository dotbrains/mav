use super::*;

impl RemoteClient {
    #[cfg(any(test, feature = "test-support"))]
    pub fn force_server_not_running(&mut self, cx: &mut Context<Self>) {
        self.set_state(State::ServerNotRunning, cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_disconnect(&self, client_cx: &mut App) -> Task<()> {
        let opts = self.connection_options();
        client_cx.spawn(async move |cx| {
            let connection = cx.update_global(|c: &mut ConnectionPool, _| {
                if let Some(ConnectionPoolEntry::Connected(c)) = c.connections.get(&opts) {
                    if let Some(connection) = c.upgrade() {
                        connection
                    } else {
                        panic!("connection was dropped")
                    }
                } else {
                    panic!("missing test connection")
                }
            });

            connection.simulate_disconnect(cx);
        })
    }

    /// Creates a mock connection pair for testing.
    ///
    /// This is the recommended way to create mock remote connections for tests.
    /// It returns the `MockConnectionOptions` (which can be passed to create a
    /// `HeadlessProject`), an `AnyProtoClient` for the server side and a
    /// `ConnectGuard` for the client side which blocks the connection from
    /// being established until dropped.
    ///
    /// # Example
    /// ```ignore
    /// let (opts, server_session, connect_guard) = RemoteClient::fake_server(cx, server_cx);
    /// // Set up HeadlessProject with server_session...
    /// drop(connect_guard);
    /// let client = RemoteClient::fake_client(opts, cx).await;
    /// ```
    #[cfg(any(test, feature = "test-support"))]
    pub fn fake_server(
        client_cx: &mut gpui::TestAppContext,
        server_cx: &mut gpui::TestAppContext,
    ) -> (RemoteConnectionOptions, AnyProtoClient, ConnectGuard) {
        use crate::transport::mock::MockConnection;
        let (opts, server_client, connect_guard) = MockConnection::new(client_cx, server_cx);
        (opts.into(), server_client, connect_guard)
    }

    /// Registers a new mock server for existing connection options.
    ///
    /// Use this to simulate reconnection: after forcing a disconnect, register
    /// a new server so the next `connect()` call succeeds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fake_server_with_opts(
        opts: &RemoteConnectionOptions,
        client_cx: &mut gpui::TestAppContext,
        server_cx: &mut gpui::TestAppContext,
    ) -> (AnyProtoClient, ConnectGuard) {
        use crate::transport::mock::MockConnection;
        let mock_opts = match opts {
            RemoteConnectionOptions::Mock(mock_opts) => mock_opts.clone(),
            _ => panic!("fake_server_with_opts requires Mock connection options"),
        };
        MockConnection::new_with_opts(mock_opts, client_cx, server_cx)
    }

    /// Creates a `RemoteClient` connected to a mock server.
    ///
    /// Call `fake_server` first to get the connection options, set up the
    /// `HeadlessProject` with the server session, then call this method
    /// to create the client.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn connect_mock(
        opts: RemoteConnectionOptions,
        client_cx: &mut gpui::TestAppContext,
    ) -> Entity<Self> {
        assert!(matches!(opts, RemoteConnectionOptions::Mock(..)));
        use crate::transport::mock::MockDelegate;
        let (_tx, rx) = oneshot::channel();
        let mut cx = client_cx.to_async();
        let connection = connect(opts, Arc::new(MockDelegate), &mut cx)
            .await
            .unwrap();
        client_cx
            .update(|cx| {
                Self::new(
                    ConnectionIdentifier::setup(),
                    connection,
                    rx,
                    Arc::new(MockDelegate),
                    cx,
                )
            })
            .await
            .unwrap()
            .unwrap()
    }

    pub fn remote_connection(&self) -> Option<Arc<dyn RemoteConnection>> {
        self.state
            .as_ref()
            .and_then(|state| state.remote_connection())
    }
}
