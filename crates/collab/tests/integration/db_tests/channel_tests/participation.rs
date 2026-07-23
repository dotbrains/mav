use super::test_support::*;
use super::*;

test_both_dbs!(
    test_user_is_channel_participant,
    test_user_is_channel_participant_postgres,
    test_user_is_channel_participant_sqlite
);

async fn test_user_is_channel_participant(db: &Arc<Database>) {
    let admin = new_test_user(db).await;
    let member = new_test_user(db).await;
    let guest = new_test_user(db).await;

    let mav_channel = db.create_root_channel("mav", admin).await.unwrap();
    let internal_channel_id = db
        .create_sub_channel("active", mav_channel, admin)
        .await
        .unwrap();
    let public_channel_id = db
        .create_sub_channel("vim", mav_channel, admin)
        .await
        .unwrap();

    db.set_channel_visibility(mav_channel, collab::db::ChannelVisibility::Public, admin)
        .await
        .unwrap();
    db.set_channel_visibility(
        public_channel_id,
        collab::db::ChannelVisibility::Public,
        admin,
    )
    .await
    .unwrap();
    db.invite_channel_member(mav_channel, member, admin, ChannelRole::Member)
        .await
        .unwrap();
    db.invite_channel_member(mav_channel, guest, admin, ChannelRole::Guest)
        .await
        .unwrap();

    db.respond_to_channel_invite(mav_channel, member, true)
        .await
        .unwrap();

    db.transaction(|tx| async move {
        db.check_user_is_channel_participant(
            &db.get_channel_internal(public_channel_id, &tx).await?,
            admin,
            &tx,
        )
        .await
    })
    .await
    .unwrap();
    db.transaction(|tx| async move {
        db.check_user_is_channel_participant(
            &db.get_channel_internal(public_channel_id, &tx).await?,
            member,
            &tx,
        )
        .await
    })
    .await
    .unwrap();

    let public_channel = db.get_channel(public_channel_id, admin).await.unwrap();
    let members = db.get_channel_members(&public_channel, 100).await.unwrap();
    let mut members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.user_id);
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: admin.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: member.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Member.into(),
            },
            proto::ChannelMember {
                user_id: guest.to_proto(),
                kind: proto::channel_member::Kind::Invitee.into(),
                role: proto::ChannelRole::Guest.into(),
            },
        ]
    );

    db.respond_to_channel_invite(mav_channel, guest, true)
        .await
        .unwrap();

    db.transaction(|tx| async move {
        db.check_user_is_channel_participant(
            &db.get_channel_internal(public_channel_id, &tx).await?,
            guest,
            &tx,
        )
        .await
    })
    .await
    .unwrap();

    let channels = db.get_channels_for_user(guest).await.unwrap().channels;
    assert_channel_tree(
        channels,
        &[(mav_channel, &[]), (public_channel_id, &[mav_channel])],
    );
    let channels = db.get_channels_for_user(member).await.unwrap().channels;
    assert_channel_tree(
        channels,
        &[
            (mav_channel, &[]),
            (internal_channel_id, &[mav_channel]),
            (public_channel_id, &[mav_channel]),
        ],
    );

    db.set_channel_member_role(mav_channel, admin, guest, ChannelRole::Banned)
        .await
        .unwrap();
    assert!(
        db.transaction(|tx| async move {
            db.check_user_is_channel_participant(
                &db.get_channel_internal(public_channel_id, &tx)
                    .await
                    .unwrap(),
                guest,
                &tx,
            )
            .await
        })
        .await
        .is_err()
    );

    let public_channel = db.get_channel(public_channel_id, admin).await.unwrap();
    let members = db.get_channel_members(&public_channel, 100).await.unwrap();
    let mut members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.user_id);
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: admin.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: member.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Member.into(),
            },
            proto::ChannelMember {
                user_id: guest.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Banned.into(),
            },
        ]
    );

    db.remove_channel_member(mav_channel, guest, admin)
        .await
        .unwrap();

    db.invite_channel_member(mav_channel, guest, admin, ChannelRole::Guest)
        .await
        .unwrap();

    // currently people invited to parent channels are not shown here
    let public_channel = db.get_channel(public_channel_id, admin).await.unwrap();
    let members = db.get_channel_members(&public_channel, 100).await.unwrap();
    let mut members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.user_id);
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: admin.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: member.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Member.into(),
            },
            proto::ChannelMember {
                user_id: guest.to_proto(),
                kind: proto::channel_member::Kind::Invitee.into(),
                role: proto::ChannelRole::Guest.into(),
            },
        ]
    );

    db.respond_to_channel_invite(mav_channel, guest, true)
        .await
        .unwrap();

    db.transaction(|tx| async move {
        db.check_user_is_channel_participant(
            &db.get_channel_internal(mav_channel, &tx).await.unwrap(),
            guest,
            &tx,
        )
        .await
    })
    .await
    .unwrap();
    assert!(
        db.transaction(|tx| async move {
            db.check_user_is_channel_participant(
                &db.get_channel_internal(internal_channel_id, &tx)
                    .await
                    .unwrap(),
                guest,
                &tx,
            )
            .await
        })
        .await
        .is_err(),
    );

    db.transaction(|tx| async move {
        db.check_user_is_channel_participant(
            &db.get_channel_internal(public_channel_id, &tx)
                .await
                .unwrap(),
            guest,
            &tx,
        )
        .await
    })
    .await
    .unwrap();

    let public_channel = db.get_channel(public_channel_id, admin).await.unwrap();
    let members = db.get_channel_members(&public_channel, 100).await.unwrap();
    let mut members = members
        .into_iter()
        .map(proto::ChannelMember::from)
        .collect::<Vec<_>>();
    members.sort_by_key(|member| member.user_id);
    assert_eq!(
        members,
        &[
            proto::ChannelMember {
                user_id: admin.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Admin.into(),
            },
            proto::ChannelMember {
                user_id: member.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Member.into(),
            },
            proto::ChannelMember {
                user_id: guest.to_proto(),
                kind: proto::channel_member::Kind::Member.into(),
                role: proto::ChannelRole::Guest.into(),
            },
        ]
    );

    let channels = db.get_channels_for_user(guest).await.unwrap().channels;
    assert_channel_tree(
        channels,
        &[(mav_channel, &[]), (public_channel_id, &[mav_channel])],
    )
}
