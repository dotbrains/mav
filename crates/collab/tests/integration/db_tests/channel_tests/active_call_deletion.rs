use super::test_support::*;
use super::*;

test_both_dbs!(
    test_delete_channel_with_active_call,
    test_delete_channel_with_active_call_postgres,
    test_delete_channel_with_active_call_sqlite
);

async fn test_delete_channel_with_active_call(db: &Arc<Database>) {
    let owner_id = db.create_server("test").await.unwrap().0 as u32;

    let user_1 = new_test_user(db).await;
    let user_2 = new_test_user(db).await;

    let parent_channel_id = db
        .create_root_channel("parent_channel", user_1)
        .await
        .unwrap();
    let nested_channel_id = db
        .create_sub_channel("nested_channel", parent_channel_id, user_1)
        .await
        .unwrap();

    db.invite_channel_member(parent_channel_id, user_2, user_1, ChannelRole::Member)
        .await
        .unwrap();

    db.respond_to_channel_invite(parent_channel_id, user_2, true)
        .await
        .unwrap();

    let connection_1 = ConnectionId { owner_id, id: 1 };
    let connection_2 = ConnectionId { owner_id, id: 2 };

    db.join_channel(parent_channel_id, user_1, connection_1)
        .await
        .unwrap();

    db.join_channel(nested_channel_id, user_2, connection_2)
        .await
        .unwrap();

    // Delete fails - participants in both parent and nested calls
    let err = db
        .delete_channel(parent_channel_id, user_1)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("call is in progress"), "{err}");

    // Delete fails - participants in nested calls
    db.leave_room(connection_2).await.unwrap();
    let err = db
        .delete_channel(parent_channel_id, user_1)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("call is in progress"), "{err}");

    // Delete succeeds - no participants in calls
    db.leave_room(connection_1).await.unwrap();
    db.delete_channel(parent_channel_id, user_1).await.unwrap();

    assert!(db.get_channel(parent_channel_id, user_1).await.is_err());
    assert!(db.get_channel(parent_channel_id, user_2).await.is_err());
}
