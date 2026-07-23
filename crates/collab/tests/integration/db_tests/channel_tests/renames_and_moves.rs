use super::test_support::*;
use super::*;

test_both_dbs!(
    test_channel_renames,
    test_channel_renames_postgres,
    test_channel_renames_sqlite
);

async fn test_channel_renames(db: &Arc<Database>) {
    db.create_server("test").await.unwrap();

    let user_1 = db.create_user(false).await.unwrap().user_id;

    let user_2 = db.create_user(false).await.unwrap().user_id;

    let mav_id = db.create_root_channel("mav", user_1).await.unwrap();

    db.rename_channel(mav_id, user_1, "#mav-archive")
        .await
        .unwrap();

    let channel = db.get_channel(mav_id, user_1).await.unwrap();
    assert_eq!(channel.name, "mav-archive");

    let non_permissioned_rename = db.rename_channel(mav_id, user_2, "hacked-lol").await;
    assert!(non_permissioned_rename.is_err());

    let bad_name_rename = db.rename_channel(mav_id, user_1, "#").await;
    assert!(bad_name_rename.is_err())
}

test_both_dbs!(
    test_db_channel_moving,
    test_db_channel_moving_postgres,
    test_db_channel_moving_sqlite
);

async fn test_db_channel_moving(db: &Arc<Database>) {
    let a_id = db.create_user(false).await.unwrap().user_id;

    let mav_id = db.create_root_channel("mav", a_id).await.unwrap();

    let crdb_id = db.create_sub_channel("crdb", mav_id, a_id).await.unwrap();

    let gpui2_id = db.create_sub_channel("gpui2", mav_id, a_id).await.unwrap();

    let livestreaming_id = db
        .create_sub_channel("livestreaming", crdb_id, a_id)
        .await
        .unwrap();

    let livestreaming_sub_id = db
        .create_sub_channel("livestreaming_sub", livestreaming_id, a_id)
        .await
        .unwrap();

    // sanity check
    //     /- gpui2
    // mav -- crdb - livestreaming - livestreaming_sub
    let result = db.get_channels_for_user(a_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (crdb_id, &[mav_id]),
            (livestreaming_id, &[mav_id, crdb_id]),
            (livestreaming_sub_id, &[mav_id, crdb_id, livestreaming_id]),
            (gpui2_id, &[mav_id]),
        ],
    );

    // Check that we can do a simple leaf -> leaf move
    db.move_channel(livestreaming_sub_id, crdb_id, a_id)
        .await
        .unwrap();

    //     /- gpui2
    // mav -- crdb -- livestreaming
    //             \- livestreaming_sub
    let result = db.get_channels_for_user(a_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (crdb_id, &[mav_id]),
            (livestreaming_id, &[mav_id, crdb_id]),
            (livestreaming_sub_id, &[mav_id, crdb_id]),
            (gpui2_id, &[mav_id]),
        ],
    );

    // Check that we can move a whole subtree at once
    db.move_channel(crdb_id, gpui2_id, a_id).await.unwrap();

    // mav -- gpui2 -- crdb -- livestreaming
    //                      \- livestreaming_sub
    let result = db.get_channels_for_user(a_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (gpui2_id, &[mav_id]),
            (crdb_id, &[mav_id, gpui2_id]),
            (livestreaming_id, &[mav_id, gpui2_id, crdb_id]),
            (livestreaming_sub_id, &[mav_id, gpui2_id, crdb_id]),
        ],
    );
}

test_both_dbs!(
    test_channel_reordering,
    test_channel_reordering_postgres,
    test_channel_reordering_sqlite
);

async fn test_channel_reordering(db: &Arc<Database>) {
    let admin_id = db.create_user(false).await.unwrap().user_id;

    let user_id = db.create_user(false).await.unwrap().user_id;

    // Create a root channel with some sub-channels
    let root_id = db.create_root_channel("root", admin_id).await.unwrap();

    // Invite user to root channel so they can see the sub-channels
    db.invite_channel_member(root_id, user_id, admin_id, ChannelRole::Member)
        .await
        .unwrap();
    db.respond_to_channel_invite(root_id, user_id, true)
        .await
        .unwrap();

    let alpha_id = db
        .create_sub_channel("alpha", root_id, admin_id)
        .await
        .unwrap();
    let beta_id = db
        .create_sub_channel("beta", root_id, admin_id)
        .await
        .unwrap();
    let gamma_id = db
        .create_sub_channel("gamma", root_id, admin_id)
        .await
        .unwrap();

    // Initial order should be: root, alpha (order=1), beta (order=2), gamma (order=3)
    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (alpha_id, &[root_id], 1),
            (beta_id, &[root_id], 2),
            (gamma_id, &[root_id], 3),
        ],
    );

    // Test moving beta up (should swap with alpha)
    let updated_channels = db
        .reorder_channel(beta_id, reorder_channel::Direction::Up, admin_id)
        .await
        .unwrap();

    // Verify that beta and alpha were returned as updated
    assert_eq!(updated_channels.len(), 2);
    let updated_ids: std::collections::HashSet<_> = updated_channels.iter().map(|c| c.id).collect();
    assert!(updated_ids.contains(&alpha_id));
    assert!(updated_ids.contains(&beta_id));

    // Now order should be: root, beta (order=1), alpha (order=2), gamma (order=3)
    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (beta_id, &[root_id], 1),
            (alpha_id, &[root_id], 2),
            (gamma_id, &[root_id], 3),
        ],
    );

    // Test moving gamma down (should be no-op since it's already last)
    let updated_channels = db
        .reorder_channel(gamma_id, reorder_channel::Direction::Down, admin_id)
        .await
        .unwrap();

    // Should return just nothing
    assert_eq!(updated_channels.len(), 0);

    // Test moving alpha down (should swap with gamma)
    let updated_channels = db
        .reorder_channel(alpha_id, reorder_channel::Direction::Down, admin_id)
        .await
        .unwrap();

    // Verify that alpha and gamma were returned as updated
    assert_eq!(updated_channels.len(), 2);
    let updated_ids: std::collections::HashSet<_> = updated_channels.iter().map(|c| c.id).collect();
    assert!(updated_ids.contains(&alpha_id));
    assert!(updated_ids.contains(&gamma_id));

    // Now order should be: root, beta (order=1), gamma (order=2), alpha (order=3)
    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (beta_id, &[root_id], 1),
            (gamma_id, &[root_id], 2),
            (alpha_id, &[root_id], 3),
        ],
    );

    // Test that non-admin cannot reorder
    let reorder_result = db
        .reorder_channel(beta_id, reorder_channel::Direction::Up, user_id)
        .await;
    assert!(reorder_result.is_err());

    // Test moving beta up (should be no-op since it's already first)
    let updated_channels = db
        .reorder_channel(beta_id, reorder_channel::Direction::Up, admin_id)
        .await
        .unwrap();

    // Should return nothing
    assert_eq!(updated_channels.len(), 0);

    // Adding a channel to an existing ordering should add it to the end
    let delta_id = db
        .create_sub_channel("delta", root_id, admin_id)
        .await
        .unwrap();

    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (beta_id, &[root_id], 1),
            (gamma_id, &[root_id], 2),
            (alpha_id, &[root_id], 3),
            (delta_id, &[root_id], 4),
        ],
    );

    // And moving a channel into an existing ordering should add it to the end
    let eta_id = db
        .create_sub_channel("eta", delta_id, admin_id)
        .await
        .unwrap();

    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (beta_id, &[root_id], 1),
            (gamma_id, &[root_id], 2),
            (alpha_id, &[root_id], 3),
            (delta_id, &[root_id], 4),
            (eta_id, &[root_id, delta_id], 1),
        ],
    );

    db.move_channel(eta_id, root_id, admin_id).await.unwrap();
    let result = db.get_channels_for_user(admin_id).await.unwrap();
    assert_channel_tree_order(
        result.channels,
        &[
            (root_id, &[], 1),
            (beta_id, &[root_id], 1),
            (gamma_id, &[root_id], 2),
            (alpha_id, &[root_id], 3),
            (delta_id, &[root_id], 4),
            (eta_id, &[root_id], 5),
        ],
    );
}

test_both_dbs!(
    test_db_channel_moving_bugs,
    test_db_channel_moving_bugs_postgres,
    test_db_channel_moving_bugs_sqlite
);

async fn test_db_channel_moving_bugs(db: &Arc<Database>) {
    let user_id = db.create_user(false).await.unwrap().user_id;

    let mav_id = db.create_root_channel("mav", user_id).await.unwrap();

    let projects_id = db
        .create_sub_channel("projects", mav_id, user_id)
        .await
        .unwrap();

    let livestreaming_id = db
        .create_sub_channel("livestreaming", projects_id, user_id)
        .await
        .unwrap();

    let result = db.get_channels_for_user(user_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (projects_id, &[mav_id]),
            (livestreaming_id, &[mav_id, projects_id]),
        ],
    );

    // Can't move a channel into its ancestor
    db.move_channel(projects_id, livestreaming_id, user_id)
        .await
        .unwrap_err();
    let result = db.get_channels_for_user(user_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (projects_id, &[mav_id]),
            (livestreaming_id, &[mav_id, projects_id]),
        ],
    );

    // Can't un-root a root channel
    db.move_channel(mav_id, livestreaming_id, user_id)
        .await
        .unwrap_err();
    let result = db.get_channels_for_user(user_id).await.unwrap();
    assert_channel_tree(
        result.channels,
        &[
            (mav_id, &[]),
            (projects_id, &[mav_id]),
            (livestreaming_id, &[mav_id, projects_id]),
        ],
    );
}
