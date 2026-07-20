use call::ActiveCall;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;

use crate::{RoomParticipants, TestServer, channel_id, room_participants};

#[gpui::test(iterations = 10)]
async fn test_calling_multiple_users_simultaneously(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_d: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_d = server.create_client(cx_d, "user_d").await;
    server
        .make_contacts(&mut [
            (&client_a, cx_a),
            (&client_b, cx_b),
            (&client_c, cx_c),
            (&client_d, cx_d),
        ])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);
    let active_call_d = cx_d.read(ActiveCall::global);

    let b_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_b.user_id().unwrap(), None, cx)
    });
    let c_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_c.user_id().unwrap(), None, cx)
    });
    b_invite.await.unwrap();
    c_invite.await.unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec!["user_b".to_string(), "user_c".to_string()]
        }
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_d.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec![
                "user_b".to_string(),
                "user_c".to_string(),
                "user_d".to_string()
            ]
        }
    );

    let accept_b = active_call_b.update(cx_b, |call, cx| call.accept_incoming(cx));
    let accept_c = active_call_c.update(cx_c, |call, cx| call.accept_incoming(cx));
    let accept_d = active_call_d.update(cx_d, |call, cx| call.accept_incoming(cx));
    accept_b.await.unwrap();
    accept_c.await.unwrap();
    accept_d.await.unwrap();

    executor.run_until_parked();

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    let room_c = active_call_c.read_with(cx_c, |call, _| call.room().unwrap().clone());
    let room_d = active_call_d.read_with(cx_d, |call, _| call.room().unwrap().clone());

    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec![
                "user_b".to_string(),
                "user_c".to_string(),
                "user_d".to_string(),
            ],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec![
                "user_a".to_string(),
                "user_c".to_string(),
                "user_d".to_string(),
            ],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec![
                "user_a".to_string(),
                "user_b".to_string(),
                "user_d".to_string(),
            ],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_d, cx_d),
        RoomParticipants {
            remote: vec![
                "user_a".to_string(),
                "user_b".to_string(),
                "user_c".to_string(),
            ],
            pending: Default::default()
        }
    );
}

#[gpui::test(iterations = 10)]
async fn test_joining_channels_and_calling_multiple_users_simultaneously(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;

    let channel_1 = server
        .make_channel(
            "channel1",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    let channel_2 = server
        .make_channel(
            "channel2",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);

    active_call_a
        .update(cx_a, |call, cx| call.join_channel(channel_1, cx))
        .detach();
    let join_channel_2 = active_call_a.update(cx_a, |call, cx| call.join_channel(channel_2, cx));

    join_channel_2.await.unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    assert_eq!(channel_id(&room_a, cx_a), Some(channel_2));

    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();

    let b_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_b.user_id().unwrap(), None, cx)
    });
    let c_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_c.user_id().unwrap(), None, cx)
    });

    let join_channel = active_call_a.update(cx_a, |call, cx| call.join_channel(channel_1, cx));

    b_invite.await.unwrap();
    c_invite.await.unwrap();
    join_channel.await.unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();

    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec!["user_b".to_string(), "user_c".to_string()]
        }
    );

    assert_eq!(channel_id(&room_a, cx_a), None);

    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();

    let join_channel = active_call_a.update(cx_a, |call, cx| call.join_channel(channel_1, cx));
    let b_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_b.user_id().unwrap(), None, cx)
    });
    let c_invite = active_call_a.update(cx_a, |call, cx| {
        call.invite(client_c.user_id().unwrap(), None, cx)
    });

    join_channel.await.unwrap();
    b_invite.await.unwrap();
    c_invite.await.unwrap();

    active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
}
