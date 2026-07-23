use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_core_channels(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    // Subscribe to channels (simulates opening the collab panel)
    client_a.initialize_channel_store(cx_a);
    client_b.initialize_channel_store(cx_b);
    executor.run_until_parked();

    let channel_a_id = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("channel-a", None, cx)
        })
        .await
        .unwrap();
    let channel_b_id = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("channel-b", Some(channel_a_id), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[
            ExpectedChannel {
                id: channel_a_id,
                name: "channel-a".into(),
                depth: 0,
            },
            ExpectedChannel {
                id: channel_b_id,
                name: "channel-b".into(),
                depth: 1,
            },
        ],
    );

    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert!(channels.ordered_channels().collect::<Vec<_>>().is_empty())
        })
    });

    // Invite client B to channel A as client A.
    client_a
        .channel_store()
        .update(cx_a, |store, cx| {
            assert!(!store.has_pending_channel_invite(channel_a_id, client_b.user_id().unwrap()));

            let invite = store.invite_member(
                channel_a_id,
                client_b.user_id().unwrap(),
                proto::ChannelRole::Member,
                cx,
            );

            // Make sure we're synchronously storing the pending invite
            assert!(store.has_pending_channel_invite(channel_a_id, client_b.user_id().unwrap()));
            invite
        })
        .await
        .unwrap();

    // Client A sees that B has been invited.
    executor.run_until_parked();
    assert_channel_invitations(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            id: channel_a_id,
            name: "channel-a".into(),
            depth: 0,
        }],
    );

    let members = client_a
        .channel_store()
        .update(cx_a, |store, cx| {
            assert!(!store.has_pending_channel_invite(channel_a_id, client_b.user_id().unwrap()));
            store.fuzzy_search_members(channel_a_id, "".to_string(), 10, cx)
        })
        .await
        .unwrap();
    assert_members_eq(
        &members,
        &[
            (
                client_a.user_id().unwrap(),
                proto::ChannelRole::Admin,
                proto::channel_member::Kind::Member,
            ),
            (
                client_b.user_id().unwrap(),
                proto::ChannelRole::Member,
                proto::channel_member::Kind::Invitee,
            ),
        ],
    );

    // Client B accepts the invitation.
    client_b
        .channel_store()
        .update(cx_b, |channels, cx| {
            channels.respond_to_channel_invite(channel_a_id, true, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // Client B now sees that they are a member of channel A and its existing subchannels.
    assert_channel_invitations(client_b.channel_store(), cx_b, &[]);
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[
            ExpectedChannel {
                id: channel_a_id,
                name: "channel-a".into(),
                depth: 0,
            },
            ExpectedChannel {
                id: channel_b_id,
                name: "channel-b".into(),
                depth: 1,
            },
        ],
    );

    let channel_c_id = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("channel-c", Some(channel_b_id), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[
            ExpectedChannel {
                id: channel_a_id,
                name: "channel-a".into(),
                depth: 0,
            },
            ExpectedChannel {
                id: channel_b_id,
                name: "channel-b".into(),
                depth: 1,
            },
            ExpectedChannel {
                id: channel_c_id,
                name: "channel-c".into(),
                depth: 2,
            },
        ],
    );

    // Update client B's membership to channel A to be an admin.
    client_a
        .channel_store()
        .update(cx_a, |store, cx| {
            store.set_member_role(
                channel_a_id,
                client_b.user_id().unwrap(),
                proto::ChannelRole::Admin,
                cx,
            )
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // Observe that client B is now an admin of channel A, and that
    // their admin privileges extend to subchannels of channel A.
    assert_channel_invitations(client_b.channel_store(), cx_b, &[]);
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[
            ExpectedChannel {
                id: channel_a_id,
                name: "channel-a".into(),
                depth: 0,
            },
            ExpectedChannel {
                id: channel_b_id,
                name: "channel-b".into(),
                depth: 1,
            },
            ExpectedChannel {
                id: channel_c_id,
                name: "channel-c".into(),
                depth: 2,
            },
        ],
    );

    // Client A deletes the channel, deletion also deletes subchannels.
    client_a
        .channel_store()
        .update(cx_a, |channel_store, _| {
            channel_store.remove_channel(channel_b_id)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[ExpectedChannel {
            id: channel_a_id,
            name: "channel-a".into(),
            depth: 0,
        }],
    );
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            id: channel_a_id,
            name: "channel-a".into(),
            depth: 0,
        }],
    );

    // Remove client B
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.remove_member(channel_a_id, client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Client A still has their channel
    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[ExpectedChannel {
            id: channel_a_id,
            name: "channel-a".into(),
            depth: 0,
        }],
    );

    // Client B no longer has access to the channel
    assert_channels(client_b.channel_store(), cx_b, &[]);

    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);

    server
        .app_state
        .db
        .rename_channel(
            db::ChannelId::from_proto(channel_a_id.0),
            UserId::from_proto(client_a.id()),
            "channel-a-renamed",
        )
        .await
        .unwrap();

    server.allow_connections();
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);

    // Re-subscribe to channels after reconnection (simulates collab panel re-rendering)
    client_a.initialize_channel_store(cx_a);
    executor.run_until_parked();

    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[ExpectedChannel {
            id: channel_a_id,
            name: "channel-a-renamed".into(),
            depth: 0,
        }],
    );
}
