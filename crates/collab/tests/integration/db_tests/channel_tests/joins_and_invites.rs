use super::test_support::*;
use super::*;

test_both_dbs!(
    test_joining_channels,
    test_joining_channels_postgres,
    test_joining_channels_sqlite
);

async fn test_joining_channels(db: &Arc<Database>) {
    let owner_id = db.create_server("test").await.unwrap().0 as u32;

    let user_1 = new_test_user(db).await;
    let user_2 = new_test_user(db).await;

    let channel_1 = db.create_root_channel("channel_1", user_1).await.unwrap();

    // can join a room with membership to its channel
    let (joined_room, _, _) = db
        .join_channel(channel_1, user_1, ConnectionId { owner_id, id: 1 })
        .await
        .unwrap();
    assert_eq!(joined_room.room.participants.len(), 1);

    let room_id = RoomId::from_proto(joined_room.room.id);
    drop(joined_room);
    // cannot join a room without membership to its channel
    assert!(
        db.join_room(room_id, user_2, ConnectionId { owner_id, id: 1 },)
            .await
            .is_err()
    );
}

test_both_dbs!(
    test_channel_invites,
    test_channel_invites_postgres,
    test_channel_invites_sqlite
);

async fn test_channel_invites(db: &Arc<Database>) {
    db.create_server("test").await.unwrap();

    let user_1 = new_test_user(db).await;
    let user_2 = new_test_user(db).await;
    let user_3 = new_test_user(db).await;

    let channel_1_1_id = db.create_root_channel("channel_1", user_1).await.unwrap();

    let channel_1_2 = db.create_root_channel("channel_2", user_1).await.unwrap();

    db.invite_channel_member(channel_1_1_id, user_2, user_1, ChannelRole::Member)
        .await
        .unwrap();
    db.invite_channel_member(channel_1_2, user_2, user_1, ChannelRole::Member)
        .await
        .unwrap();
    db.invite_channel_member(channel_1_1_id, user_3, user_1, ChannelRole::Admin)
        .await
        .unwrap();

    let user_2_invites = db
        .get_channels_for_user(user_2)
        .await
        .unwrap()
        .invited_channels
        .into_iter()
        .map(|channel| channel.id)
        .collect::<Vec<_>>();
    assert_eq!(user_2_invites, &[channel_1_1_id, channel_1_2]);

    let user_3_invites = db
        .get_channels_for_user(user_3)
        .await
        .unwrap()
        .invited_channels
        .into_iter()
        .map(|channel| channel.id)
        .collect::<Vec<_>>();
    assert_eq!(user_3_invites, &[channel_1_1_id]);

    let channel_1_1 = db.get_channel(channel_1_1_id, user_1).await.unwrap();
    let members = db.get_channel_members(&channel_1_1, 100).await.unwrap();
    let mut members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.user_id);
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: user_1.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: user_2.to_proto(),
                kind: proto::channel_member::Kind::Invitee.into(),
                role: proto::ChannelRole::Member.into(),
            },
            proto::ChannelMember {
                user_id: user_3.to_proto(),
                kind: proto::channel_member::Kind::Invitee.into(),
                role: proto::ChannelRole::Admin.into(),
            },
        ]
    );

    db.respond_to_channel_invite(channel_1_1_id, user_2, true)
        .await
        .unwrap();

    let channel_1_3_id = db
        .create_sub_channel("channel_3", channel_1_1_id, user_1)
        .await
        .unwrap();

    let channel_1_3 = db.get_channel(channel_1_3_id, user_1).await.unwrap();
    let members = db.get_channel_members(&channel_1_3, 100).await.unwrap();
    let members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: user_1.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: user_3.to_proto(),
                kind: proto::channel_member::Kind::Invitee.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: user_2.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Member.into(),
            },
        ]
    );
}
