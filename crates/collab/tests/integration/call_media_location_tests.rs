use call::{ActiveCall, Room, room};
use client::User;
use gpui::{BackgroundExecutor, Entity, TestAppContext};
use pretty_assertions::assert_eq;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    mem,
    rc::Rc,
    sync::Arc,
};
use workspace::ParticipantLocation;

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_active_call_events(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    client_a.fs().insert_tree("/a", json!({})).await;
    client_b.fs().insert_tree("/b", json!({})).await;

    let (project_a, _) = client_a.build_local_project("/a", cx_a).await;
    let (project_b, _) = client_b.build_local_project("/b", cx_b).await;

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    executor.run_until_parked();

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let events_a = active_call_events(cx_a);
    let events_b = active_call_events(cx_b);

    let project_a_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(mem::take(&mut *events_a.borrow_mut()), vec![]);
    assert_eq!(
        mem::take(&mut *events_b.borrow_mut()),
        vec![room::Event::RemoteProjectShared {
            owner: Arc::new(User {
                legacy_id: client_a.user_id().unwrap(),
                username: "user_a".into(),
                avatar_uri: "avatar_a".into(),
                name: None,
            }),
            project_id: project_a_id,
            worktree_root_names: vec!["a".to_string()],
        }]
    );

    let project_b_id = active_call_b
        .update(cx_b, |call, cx| call.share_project(project_b.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        mem::take(&mut *events_a.borrow_mut()),
        vec![room::Event::RemoteProjectShared {
            owner: Arc::new(User {
                legacy_id: client_b.user_id().unwrap(),
                username: "user_b".into(),
                avatar_uri: "avatar_b".into(),
                name: None,
            }),
            project_id: project_b_id,
            worktree_root_names: vec!["b".to_string()]
        }]
    );
    assert_eq!(mem::take(&mut *events_b.borrow_mut()), vec![]);

    // Sharing a project twice is idempotent.
    let project_b_id_2 = active_call_b
        .update(cx_b, |call, cx| call.share_project(project_b.clone(), cx))
        .await
        .unwrap();
    assert_eq!(project_b_id_2, project_b_id);
    executor.run_until_parked();
    assert_eq!(mem::take(&mut *events_a.borrow_mut()), vec![]);
    assert_eq!(mem::take(&mut *events_b.borrow_mut()), vec![]);

    // Unsharing a project should dispatch the RemoteProjectUnshared event.
    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    assert_eq!(
        mem::take(&mut *events_a.borrow_mut()),
        vec![room::Event::RoomLeft { channel_id: None }]
    );
    assert_eq!(
        mem::take(&mut *events_b.borrow_mut()),
        vec![room::Event::RemoteProjectUnshared {
            project_id: project_a_id,
        }]
    );
}

fn active_call_events(cx: &mut TestAppContext) -> Rc<RefCell<Vec<room::Event>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let active_call = cx.read(ActiveCall::global);
    cx.update({
        let events = events.clone();
        |cx| {
            cx.subscribe(&active_call, move |_, event, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach()
        }
    });
    events
}

#[gpui::test]
async fn test_mute_deafen(
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

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);

    // User A calls user B, B answers.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());
    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());

    room_a.read_with(cx_a, |room, _| assert!(!room.is_muted()));
    room_b.read_with(cx_b, |room, _| assert!(!room.is_muted()));

    // Users A and B are both unmuted.
    assert_eq!(
        participant_audio_state(&room_a, cx_a),
        &[ParticipantAudioState {
            user_id: client_b.user_id().unwrap(),
            is_muted: false,
            audio_tracks_playing: vec![true],
        }]
    );
    assert_eq!(
        participant_audio_state(&room_b, cx_b),
        &[ParticipantAudioState {
            user_id: client_a.user_id().unwrap(),
            is_muted: false,
            audio_tracks_playing: vec![true],
        }]
    );

    // User A mutes
    room_a.update(cx_a, |room, cx| room.toggle_mute(cx));
    executor.run_until_parked();

    // User A hears user B, but B doesn't hear A.
    room_a.read_with(cx_a, |room, _| assert!(room.is_muted()));
    room_b.read_with(cx_b, |room, _| assert!(!room.is_muted()));
    assert_eq!(
        participant_audio_state(&room_a, cx_a),
        &[ParticipantAudioState {
            user_id: client_b.user_id().unwrap(),
            is_muted: false,
            audio_tracks_playing: vec![true],
        }]
    );
    assert_eq!(
        participant_audio_state(&room_b, cx_b),
        &[ParticipantAudioState {
            user_id: client_a.user_id().unwrap(),
            is_muted: true,
            audio_tracks_playing: vec![true],
        }]
    );

    // User A deafens
    room_a.update(cx_a, |room, cx| room.toggle_deafen(cx));
    executor.run_until_parked();

    // User A does not hear user B.
    room_a.read_with(cx_a, |room, _| assert!(room.is_muted()));
    room_b.read_with(cx_b, |room, _| assert!(!room.is_muted()));
    assert_eq!(
        participant_audio_state(&room_a, cx_a),
        &[ParticipantAudioState {
            user_id: client_b.user_id().unwrap(),
            is_muted: false,
            audio_tracks_playing: vec![false],
        }]
    );
    assert_eq!(
        participant_audio_state(&room_b, cx_b),
        &[ParticipantAudioState {
            user_id: client_a.user_id().unwrap(),
            is_muted: true,
            audio_tracks_playing: vec![true],
        }]
    );

    // User B calls user C, C joins.
    active_call_b
        .update(cx_b, |call, cx| {
            call.invite(client_c.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    active_call_c
        .update(cx_c, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    // User A does not hear users B or C.
    assert_eq!(
        participant_audio_state(&room_a, cx_a),
        &[
            ParticipantAudioState {
                user_id: client_b.user_id().unwrap(),
                is_muted: false,
                audio_tracks_playing: vec![false],
            },
            ParticipantAudioState {
                user_id: client_c.user_id().unwrap(),
                is_muted: false,
                audio_tracks_playing: vec![false],
            }
        ]
    );
    assert_eq!(
        participant_audio_state(&room_b, cx_b),
        &[
            ParticipantAudioState {
                user_id: client_a.user_id().unwrap(),
                is_muted: true,
                audio_tracks_playing: vec![true],
            },
            ParticipantAudioState {
                user_id: client_c.user_id().unwrap(),
                is_muted: false,
                audio_tracks_playing: vec![true],
            }
        ]
    );

    #[derive(PartialEq, Eq, Debug)]
    struct ParticipantAudioState {
        user_id: u64,
        is_muted: bool,
        audio_tracks_playing: Vec<bool>,
    }

    fn participant_audio_state(
        room: &Entity<Room>,
        cx: &TestAppContext,
    ) -> Vec<ParticipantAudioState> {
        room.read_with(cx, |room, _| {
            room.remote_participants()
                .iter()
                .map(|(user_id, participant)| ParticipantAudioState {
                    user_id: *user_id,
                    is_muted: participant.muted,
                    audio_tracks_playing: participant
                        .audio_tracks
                        .values()
                        .map(|(track, _)| track.enabled())
                        .collect(),
                })
                .collect::<Vec<_>>()
        })
    }
}

#[gpui::test(iterations = 10)]
async fn test_room_location(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    client_a.fs().insert_tree("/a", json!({})).await;
    client_b.fs().insert_tree("/b", json!({})).await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let a_notified = Rc::new(Cell::new(false));
    cx_a.update({
        let notified = a_notified.clone();
        |cx| {
            cx.observe(&active_call_a, move |_, _| notified.set(true))
                .detach()
        }
    });

    let b_notified = Rc::new(Cell::new(false));
    cx_b.update({
        let b_notified = b_notified.clone();
        |cx| {
            cx.observe(&active_call_b, move |_, _| b_notified.set(true))
                .detach()
        }
    });

    let (project_a, _) = client_a.build_local_project("/a", cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();
    let (project_b, _) = client_b.build_local_project("/b", cx_b).await;

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let room_a = active_call_a.read_with(cx_a, |call, _| call.room().unwrap().clone());

    let room_b = active_call_b.read_with(cx_b, |call, _| call.room().unwrap().clone());
    executor.run_until_parked();
    assert!(a_notified.take());
    assert_eq!(
        participant_locations(&room_a, cx_a),
        vec![("user_b".to_string(), ParticipantLocation::External)]
    );
    assert!(b_notified.take());
    assert_eq!(
        participant_locations(&room_b, cx_b),
        vec![("user_a".to_string(), ParticipantLocation::UnsharedProject)]
    );

    let project_a_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(a_notified.take());
    assert_eq!(
        participant_locations(&room_a, cx_a),
        vec![("user_b".to_string(), ParticipantLocation::External)]
    );
    assert!(b_notified.take());
    assert_eq!(
        participant_locations(&room_b, cx_b),
        vec![(
            "user_a".to_string(),
            ParticipantLocation::SharedProject {
                project_id: project_a_id
            }
        )]
    );

    let project_b_id = active_call_b
        .update(cx_b, |call, cx| call.share_project(project_b.clone(), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(a_notified.take());
    assert_eq!(
        participant_locations(&room_a, cx_a),
        vec![("user_b".to_string(), ParticipantLocation::External)]
    );
    assert!(b_notified.take());
    assert_eq!(
        participant_locations(&room_b, cx_b),
        vec![(
            "user_a".to_string(),
            ParticipantLocation::SharedProject {
                project_id: project_a_id
            }
        )]
    );

    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(a_notified.take());
    assert_eq!(
        participant_locations(&room_a, cx_a),
        vec![(
            "user_b".to_string(),
            ParticipantLocation::SharedProject {
                project_id: project_b_id
            }
        )]
    );
    assert!(b_notified.take());
    assert_eq!(
        participant_locations(&room_b, cx_b),
        vec![(
            "user_a".to_string(),
            ParticipantLocation::SharedProject {
                project_id: project_a_id
            }
        )]
    );

    active_call_b
        .update(cx_b, |call, cx| call.set_location(None, cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(a_notified.take());
    assert_eq!(
        participant_locations(&room_a, cx_a),
        vec![("user_b".to_string(), ParticipantLocation::External)]
    );
    assert!(b_notified.take());
    assert_eq!(
        participant_locations(&room_b, cx_b),
        vec![(
            "user_a".to_string(),
            ParticipantLocation::SharedProject {
                project_id: project_a_id
            }
        )]
    );

    fn participant_locations(
        room: &Entity<Room>,
        cx: &TestAppContext,
    ) -> Vec<(String, ParticipantLocation)> {
        room.read_with(cx, |room, _| {
            room.remote_participants()
                .values()
                .map(|participant| (participant.user.username.to_string(), participant.location))
                .collect()
        })
    }
}
