use super::test_support::*;
use super::*;

test_both_dbs!(test_channels, test_channels_postgres, test_channels_sqlite);

async fn test_channels(db: &Arc<Database>) {
    let a_id = new_test_user(db).await;
    let b_id = new_test_user(db).await;

    let mav_id = db.create_root_channel("mav", a_id).await.unwrap();

    // Make sure that people cannot read channels they haven't been invited to
    assert!(db.get_channel(mav_id, b_id).await.is_err());

    db.invite_channel_member(mav_id, b_id, a_id, ChannelRole::Member)
        .await
        .unwrap();

    db.respond_to_channel_invite(mav_id, b_id, true)
        .await
        .unwrap();

    let crdb_id = db.create_sub_channel("crdb", mav_id, a_id).await.unwrap();
    let livestreaming_id = db
        .create_sub_channel("livestreaming", mav_id, a_id)
        .await
        .unwrap();
    let replace_id = db
        .create_sub_channel("replace", mav_id, a_id)
        .await
        .unwrap();

    let replace_channel = db.get_channel(replace_id, a_id).await.unwrap();
    let members = db.get_channel_members(&replace_channel, 10).await.unwrap();
    let ids = members.into_iter().map(|m| m.user_id).collect::<Vec<_>>();
    assert_eq!(ids, &[a_id, b_id]);

    let rust_id = db.create_root_channel("rust", a_id).await.unwrap();
    let cargo_id = db.create_sub_channel("cargo", rust_id, a_id).await.unwrap();

    let cargo_ra_id = db
        .create_sub_channel("cargo-ra", cargo_id, a_id)
        .await
        .unwrap();

    let result = db.get_channels_for_user(a_id).await.unwrap();
    assert_channel_tree_matches(
        result.channels,
        channel_tree(&[
            (mav_id, &[], "mav"),
            (crdb_id, &[mav_id], "crdb"),
            (livestreaming_id, &[mav_id], "livestreaming"),
            (replace_id, &[mav_id], "replace"),
            (rust_id, &[], "rust"),
            (cargo_id, &[rust_id], "cargo"),
            (cargo_ra_id, &[rust_id, cargo_id], "cargo-ra"),
        ]),
    );

    let result = db.get_channels_for_user(b_id).await.unwrap();
    assert_channel_tree_matches(
        result.channels,
        channel_tree(&[
            (mav_id, &[], "mav"),
            (crdb_id, &[mav_id], "crdb"),
            (livestreaming_id, &[mav_id], "livestreaming"),
            (replace_id, &[mav_id], "replace"),
        ]),
    );

    // Update member permissions
    let set_subchannel_admin = db
        .set_channel_member_role(crdb_id, a_id, b_id, ChannelRole::Admin)
        .await;
    assert!(set_subchannel_admin.is_err());
    let set_channel_admin = db
        .set_channel_member_role(mav_id, a_id, b_id, ChannelRole::Admin)
        .await;
    assert!(set_channel_admin.is_ok());

    let result = db.get_channels_for_user(b_id).await.unwrap();
    assert_channel_tree_matches(
        result.channels,
        channel_tree(&[
            (mav_id, &[], "mav"),
            (crdb_id, &[mav_id], "crdb"),
            (livestreaming_id, &[mav_id], "livestreaming"),
            (replace_id, &[mav_id], "replace"),
        ]),
    );

    // Remove a single channel
    db.delete_channel(crdb_id, a_id).await.unwrap();
    assert!(db.get_channel(crdb_id, a_id).await.is_err());

    // Remove a channel tree
    let (_, mut channel_ids) = db.delete_channel(rust_id, a_id).await.unwrap();
    channel_ids.sort();
    assert_eq!(channel_ids, &[rust_id, cargo_id, cargo_ra_id]);

    assert!(db.get_channel(rust_id, a_id).await.is_err());
    assert!(db.get_channel(cargo_id, a_id).await.is_err());
    assert!(db.get_channel(cargo_ra_id, a_id).await.is_err());
}
