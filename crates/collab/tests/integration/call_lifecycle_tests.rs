use call::{ActiveCall, room};
use client::RECEIVE_TIMEOUT;
use collab::rpc::RECONNECT_TIMEOUT;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use std::{cell::RefCell, rc::Rc};

use crate::{RoomParticipants, TestServer, room_participants};

#[gpui::test(iterations = 10)]
async fn test_database_failure_during_client_reconnection(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client = server.create_client(cx, "user_a").await;

    server.test_db.set_query_failure_probability(0.3);
    loop {
        server.disconnect_client(client.peer_id().unwrap());
        executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
        if !client.status().borrow().is_connected() {
            break;
        }
    }

    server.test_db.set_query_failure_probability(0.);
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert!(
        matches!(*client.status().borrow(), client::Status::Connected { .. }),
        "status was {:?}",
        *client.status().borrow()
    );
}

#[gpui::test(iterations = 10)]
async fn test_basic_calls(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_b2: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: vec!["user_b".to_string()]
        }
    );

    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    let call_b = incoming_call_b.next().await.unwrap().unwrap();
    assert_eq!(call_b.calling_user.username, "user_a");

    let _client_b2 = server.create_client(cx_b2, "user_b").await;
    let active_call_b2 = cx_b2.read(ActiveCall::global);

    let mut incoming_call_b2 = active_call_b2.read_with(cx_b2, |call, _| call.incoming());
    executor.run_until_parked();
    let call_b2 = incoming_call_b2.next().await.unwrap().unwrap();
    assert_eq!(call_b2.calling_user.username, "user_a");

    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    assert!(incoming_call_b.next().await.unwrap().is_none());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: Default::default()
        }
    );

    let mut incoming_call_c = active_call_c.read_with(cx_c, |call, _| call.incoming());
    active_call_b
        .update(cx_b, |call, cx| {
            call.invite(client_c.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec!["user_c".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec!["user_c".to_string()]
        }
    );

    let call_c = incoming_call_c.next().await.unwrap().unwrap();
    assert_eq!(call_c.calling_user.username, "user_b");
    active_call_c.update(cx_c, |call, cx| call.decline_incoming(cx).unwrap());
    assert!(incoming_call_c.next().await.unwrap().is_none());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: Default::default()
        }
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_c.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec!["user_c".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec!["user_c".to_string()]
        }
    );

    let call_c = incoming_call_c.next().await.unwrap().unwrap();
    assert_eq!(call_c.calling_user.username, "user_a");
    active_call_c
        .update(cx_c, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    assert!(incoming_call_c.next().await.unwrap().is_none());

    let room_c = active_call_c.read_with(cx_c, |call, _| call.room().unwrap().clone());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string(), "user_c".to_string()],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_c".to_string()],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_b".to_string()],
            pending: Default::default()
        }
    );

    let display = gpui::TestScreenCaptureSource::new();
    let events_b = active_call_events(cx_b);
    let events_c = active_call_events(cx_c);
    cx_a.set_screen_capture_sources(vec![display]);
    let screen_a = cx_a
        .update(|cx| cx.screen_capture_sources())
        .await
        .unwrap()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    active_call_a
        .update(cx_a, |call, cx| {
            call.room()
                .unwrap()
                .update(cx, |room, cx| room.share_screen(screen_a, cx))
        })
        .await
        .unwrap();

    executor.run_until_parked();

    assert_eq!(events_b.borrow().len(), 1);
    let event_b = events_b.borrow().first().unwrap().clone();
    if let call::room::Event::RemoteVideoTracksChanged { participant_id } = event_b {
        assert_eq!(participant_id, client_a.peer_id().unwrap());

        room_b.read_with(cx_b, |room, _| {
            assert_eq!(
                room.remote_participants()[&client_a.user_id().unwrap()]
                    .video_tracks
                    .len(),
                1
            );
        });
    } else {
        panic!("unexpected event")
    }

    assert_eq!(events_c.borrow().len(), 1);
    let event_c = events_c.borrow().first().unwrap().clone();
    if let call::room::Event::RemoteVideoTracksChanged { participant_id } = event_c {
        assert_eq!(participant_id, client_a.peer_id().unwrap());

        room_c.read_with(cx_c, |room, _| {
            assert_eq!(
                room.remote_participants()[&client_a.user_id().unwrap()]
                    .video_tracks
                    .len(),
                1
            );
        });
    } else {
        panic!("unexpected event")
    }

    active_call_a
        .update(cx_a, |call, cx| {
            let hang_up = call.hang_up(cx);
            assert!(call.room().is_none());
            hang_up
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_c".to_string()],
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: Default::default()
        }
    );

    server
        .test_livekit_server
        .disconnect_client(client_b.user_id().unwrap().to_string())
        .await;
    executor.run_until_parked();

    active_call_b.read_with(cx_b, |call, _| assert!(call.room().is_none()));

    active_call_c.read_with(cx_c, |call, _| assert!(call.room().is_none()));
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: Default::default(),
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: Default::default(),
            pending: Default::default()
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: Default::default(),
            pending: Default::default()
        }
    );
}

fn active_call_events(cx: &mut TestAppContext) -> Rc<RefCell<Vec<room::Event>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    cx.update(|cx| {
        let active_call = ActiveCall::global(cx);
        let events = events.clone();
        let subscription = active_call.update(cx, |call, cx| {
            cx.subscribe(&call.room().unwrap(), move |_, _, event, _| {
                events.borrow_mut().push(event.clone());
            })
        });
        subscription.detach();
    });
    events
}
