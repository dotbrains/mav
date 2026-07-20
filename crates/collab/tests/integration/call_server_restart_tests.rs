use call::ActiveCall;
use collab::rpc::{CLEANUP_TIMEOUT, RECONNECT_TIMEOUT};
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use std::future;

use crate::{RoomParticipants, TestServer, room_participants};

#[gpui::test(iterations = 10)]
async fn test_server_restarts(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_d: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    client_a
        .fs()
        .insert_tree("/a", json!({ "a.txt": "a-contents" }))
        .await;

    // Invite client B to collaborate on a project
    let (project_a, _) = client_a.build_local_project("/a", cx_a).await;

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

    // User A calls users B, C, and D.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), Some(project_a.clone()), cx)
        })
        .await
        .unwrap();
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_c.user_id().unwrap(), Some(project_a.clone()), cx)
        })
        .await
        .unwrap();
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_d.user_id().unwrap(), Some(project_a.clone()), cx)
        })
        .await
        .unwrap();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());

    // User B receives the call and joins the room.

    let mut incoming_call_b = active_call_b.read_with(cx_b, |call, _| call.incoming());
    assert!(incoming_call_b.next().await.unwrap().is_some());
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());

    // User C receives the call and joins the room.

    let mut incoming_call_c = active_call_c.read_with(cx_c, |call, _| call.incoming());
    assert!(incoming_call_c.next().await.unwrap().is_some());
    active_call_c
        .update(cx_c, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    let room_c = active_call_c.read_with(cx_c, |call, _| call.room().unwrap().clone());

    // User D receives the call but doesn't join the room yet.

    let mut incoming_call_d = active_call_d.read_with(cx_d, |call, _| call.incoming());
    assert!(incoming_call_d.next().await.unwrap().is_some());

    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string(), "user_c".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_c".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_b".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );

    // The server is torn down.
    server.reset().await;

    // Users A and B reconnect to the call. User C has troubles reconnecting, so it leaves the room.
    client_c.override_establish_connection(|_, cx| cx.spawn(async |_| future::pending().await));
    executor.advance_clock(RECONNECT_TIMEOUT);
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string(), "user_c".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_c".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );

    // User D is notified again of the incoming call and accepts it.
    assert!(incoming_call_d.next().await.unwrap().is_some());
    active_call_d
        .update(cx_d, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let room_d = active_call_d.read_with(cx_d, |call, _| call.room().unwrap().clone());
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec![
                "user_b".to_string(),
                "user_c".to_string(),
                "user_d".to_string(),
            ],
            pending: vec![]
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
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
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
            pending: vec![]
        }
    );

    // The server finishes restarting, cleaning up stale connections.
    server.start().await.unwrap();
    executor.advance_clock(CLEANUP_TIMEOUT);
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string(), "user_d".to_string()],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_d".to_string()],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_d, cx_d),
        RoomParticipants {
            remote: vec!["user_a".to_string(), "user_b".to_string()],
            pending: vec![]
        }
    );

    // User D hangs up.
    active_call_d
        .update(cx_d, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_c, cx_c),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_d, cx_d),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );

    // User B calls user D again.
    active_call_b
        .update(cx_b, |call, cx| {
            call.invite(client_d.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // User D receives the call but doesn't join the room yet.

    let mut incoming_call_d = active_call_d.read_with(cx_d, |call, _| call.incoming());
    assert!(incoming_call_d.next().await.unwrap().is_some());
    executor.run_until_parked();
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec!["user_b".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec!["user_a".to_string()],
            pending: vec!["user_d".to_string()]
        }
    );

    // The server is torn down.
    server.reset().await;

    // Users A and B have troubles reconnecting, so they leave the room.
    client_a.override_establish_connection(|_, cx| cx.spawn(async |_| future::pending().await));
    client_b.override_establish_connection(|_, cx| cx.spawn(async |_| future::pending().await));
    client_c.override_establish_connection(|_, cx| cx.spawn(async |_| future::pending().await));
    executor.advance_clock(RECONNECT_TIMEOUT);
    assert_eq!(
        room_participants(&room_a, cx_a),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );
    assert_eq!(
        room_participants(&room_b, cx_b),
        RoomParticipants {
            remote: vec![],
            pending: vec![]
        }
    );

    // User D is notified again of the incoming call but doesn't accept it.
    assert!(incoming_call_d.next().await.unwrap().is_some());

    // The server finishes restarting, cleaning up stale connections and canceling the
    // call to user D because the room has become empty.
    server.start().await.unwrap();
    executor.advance_clock(CLEANUP_TIMEOUT);
    assert!(incoming_call_d.next().await.unwrap().is_none());
}
