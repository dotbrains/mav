use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_room(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    let mav_id = server
        .make_channel(
            "mav",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    active_call_a
        .update(cx_a, |active_call, cx| active_call.join_channel(mav_id, cx))
        .await
        .unwrap();

    // Give everyone a chance to observe user A joining
    executor.run_until_parked();
    let room_a =
        cx_a.read(|cx| active_call_a.read_with(cx, |call, _| call.room().unwrap().clone()));
    cx_a.read(|cx| room_a.read_with(cx, |room, cx| assert!(room.is_connected(cx))));

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap()],
            );
        })
    });

    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            id: mav_id,
            name: "mav".into(),
            depth: 0,
        }],
    );
    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap()],
            );
        })
    });

    cx_c.read(|cx| {
        client_c.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap()],
            );
        })
    });

    active_call_b
        .update(cx_b, |active_call, cx| active_call.join_channel(mav_id, cx))
        .await
        .unwrap();

    executor.run_until_parked();

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });

    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });

    cx_c.read(|cx| {
        client_c.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });

    let room_a =
        cx_a.read(|cx| active_call_a.read_with(cx, |call, _| call.room().unwrap().clone()));
    cx_a.read(|cx| room_a.read_with(cx, |room, cx| assert!(room.is_connected(cx))));
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec![]
        }
    );

    let room_b =
        cx_b.read(|cx| active_call_b.read_with(cx, |call, _| call.room().unwrap().clone()));
    cx_b.read(|cx| room_b.read_with(cx, |room, cx| assert!(room.is_connected(cx))));
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec![]
        }
    );

    // Make sure that leaving and rejoining works

    active_call_a
        .update(cx_a, |active_call, cx| active_call.hang_up(cx))
        .await
        .unwrap();

    executor.run_until_parked();

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_b.user_id().unwrap()],
            );
        })
    });

    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_b.user_id().unwrap()],
            );
        })
    });

    cx_c.read(|cx| {
        client_c.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_b.user_id().unwrap()],
            );
        })
    });

    active_call_b
        .update(cx_b, |active_call, cx| active_call.hang_up(cx))
        .await
        .unwrap();

    executor.run_until_parked();

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(channels.channel_participants(mav_id), &[]);
        })
    });

    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(channels.channel_participants(mav_id), &[]);
        })
    });

    cx_c.read(|cx| {
        client_c.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(channels.channel_participants(mav_id), &[]);
        })
    });

    active_call_a
        .update(cx_a, |active_call, cx| active_call.join_channel(mav_id, cx))
        .await
        .unwrap();

    active_call_b
        .update(cx_b, |active_call, cx| active_call.join_channel(mav_id, cx))
        .await
        .unwrap();

    executor.run_until_parked();

    let room_a =
        cx_a.read(|cx| active_call_a.read_with(cx, |call, _| call.room().unwrap().clone()));
    cx_a.read(|cx| room_a.read_with(cx, |room, cx| assert!(room.is_connected(cx))));
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec![]
        }
    );

    let room_b =
        cx_b.read(|cx| active_call_b.read_with(cx, |call, _| call.room().unwrap().clone()));
    cx_b.read(|cx| room_b.read_with(cx, |room, cx| assert!(room.is_connected(cx))));
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec![]
        }
    );
}
