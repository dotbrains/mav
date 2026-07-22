#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use rpc::{ErrorCodeExt, proto::ErrorCode};

    #[test]
    fn test_ssh_display_name_prefers_nickname() {
        let options = RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: "1.2.3.4".into(),
            nickname: Some("My Cool Project".to_string()),
            ..Default::default()
        });

        assert_eq!(options.display_name(), "My Cool Project");
    }

    #[test]
    fn test_ssh_display_name_falls_back_to_host() {
        let options = RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: "1.2.3.4".into(),
            ..Default::default()
        });

        assert_eq!(options.display_name(), "1.2.3.4");
    }

    #[test]
    fn test_connection_type() {
        assert_eq!(
            RemoteConnectionOptions::Ssh(SshConnectionOptions::default()).connection_type(),
            "ssh"
        );
        assert_eq!(
            RemoteConnectionOptions::Wsl(WslConnectionOptions {
                distro_name: "Ubuntu".to_string(),
                user: None,
            })
            .connection_type(),
            "wsl"
        );
        assert_eq!(
            RemoteConnectionOptions::Docker(DockerConnectionOptions {
                use_podman: false,
                ..Default::default()
            })
            .connection_type(),
            "docker"
        );
        assert_eq!(
            RemoteConnectionOptions::Docker(DockerConnectionOptions {
                use_podman: true,
                ..Default::default()
            })
            .connection_type(),
            "podman"
        );
    }

    #[gpui::test]
    async fn test_channel_client_request_stream_terminates_on_error(cx: &mut TestAppContext) {
        let (incoming_tx, incoming_rx) = mpsc::unbounded::<Envelope>();
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded::<Envelope>();

        let client =
            cx.update(|cx| ChannelClient::new(incoming_rx, outgoing_tx, cx, "test-client", false));

        // The client sends RemoteStarted on startup; drain the outgoing channel
        // so it doesn't block.
        let _drain_outgoing = cx
            .executor()
            .spawn(async move { while outgoing_rx.next().await.is_some() {} });

        let mut stream = client
            .request_stream_dynamic(proto::Test { id: 0 }.into_envelope(0, None, None), "Test")
            .await
            .unwrap();

        let request_id = 0;

        incoming_tx
            .unbounded_send(proto::Test { id: 1 }.into_envelope(100, Some(request_id), None))
            .unwrap();

        let first = stream.next().await.unwrap().unwrap();
        assert_eq!(
            proto::Test::from_envelope(first).unwrap(),
            proto::Test { id: 1 }
        );

        // Send an Error without a trailing EndStream. The Error alone should
        // terminate the stream.
        incoming_tx
            .unbounded_send(
                ErrorCode::Internal
                    .message("boom".to_string())
                    .to_proto()
                    .into_envelope(101, Some(request_id), None),
            )
            .unwrap();

        let second = stream.next().await.unwrap();
        let error = second.unwrap_err();
        assert!(
            format!("{error}").contains("boom"),
            "expected error to surface server message, got: {error}"
        );

        assert!(stream.next().await.is_none());
        assert_eq!(client.stream_response_channels.lock().len(), 0);
    }

    #[gpui::test]
    async fn test_channel_client_dropping_stream_request_before_response_cleans_up_channel(
        cx: &mut TestAppContext,
    ) {
        let (_incoming_tx, incoming_rx) = mpsc::unbounded::<Envelope>();
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded::<Envelope>();

        let client =
            cx.update(|cx| ChannelClient::new(incoming_rx, outgoing_tx, cx, "test-client", false));

        let _drain_outgoing = cx
            .executor()
            .spawn(async move { while outgoing_rx.next().await.is_some() {} });

        let stream = client
            .request_stream_dynamic(proto::Test { id: 0 }.into_envelope(0, None, None), "Test")
            .await
            .unwrap();

        assert_eq!(client.stream_response_channels.lock().len(), 1);

        drop(stream);
        cx.run_until_parked();

        assert_eq!(
            client.stream_response_channels.lock().len(),
            0,
            "dropping a stream before any responses arrive should remove response channel bookkeeping"
        );
    }

    #[gpui::test]
    async fn test_channel_client_dropping_stream_request_before_completion(
        cx: &mut TestAppContext,
    ) {
        let (incoming_tx, incoming_rx) = mpsc::unbounded::<Envelope>();
        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded::<Envelope>();

        let client =
            cx.update(|cx| ChannelClient::new(incoming_rx, outgoing_tx, cx, "test-client", false));

        let _drain_outgoing = cx
            .executor()
            .spawn(async move { while outgoing_rx.next().await.is_some() {} });

        let mut stream = client
            .request_stream_dynamic(proto::Test { id: 0 }.into_envelope(0, None, None), "Test")
            .await
            .unwrap();

        let request_id = 0;

        incoming_tx
            .unbounded_send(proto::Test { id: 1 }.into_envelope(100, Some(request_id), None))
            .unwrap();
        let _ = stream.next().await.unwrap().unwrap();

        assert_eq!(client.stream_response_channels.lock().len(), 1);

        drop(stream);

        // Inject an orphaned non-terminal response. The read loop should detect
        // that the consumer has been dropped and clean up its bookkeeping (no
        // EndStream sent here on purpose, otherwise the cleanup would happen
        // via the terminal-response path and mask the bug under test).
        incoming_tx
            .unbounded_send(proto::Test { id: 2 }.into_envelope(101, Some(request_id), None))
            .unwrap();

        cx.run_until_parked();

        assert_eq!(
            client.stream_response_channels.lock().len(),
            0,
            "stream channel should be removed once the consumer has dropped the stream"
        );
    }
}
