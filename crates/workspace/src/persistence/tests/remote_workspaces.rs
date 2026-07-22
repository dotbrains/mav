use super::*;

#[gpui::test]
async fn test_get_or_create_ssh_project() {
    let db = WorkspaceDb::open_test_db("test_get_or_create_ssh_project").await;

    let host = "example.com".to_string();
    let port = Some(22_u16);
    let user = Some("user".to_string());

    let connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    // Test that calling the function again with the same parameters returns the same project
    let same_connection = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_eq!(connection_id, same_connection);

    // Test with different parameters
    let host2 = "otherexample.com".to_string();
    let port2 = None;
    let user2 = Some("otheruser".to_string());

    let different_connection = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host2.clone().into(),
            port: port2,
            username: user2.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_ne!(connection_id, different_connection);
}

#[gpui::test]
async fn test_get_or_create_ssh_project_with_null_user() {
    let db = WorkspaceDb::open_test_db("test_get_or_create_ssh_project_with_null_user").await;

    let (host, port, user) = ("example.com".to_string(), None, None);

    let connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: None,
            ..Default::default()
        }))
        .await
        .unwrap();

    let same_connection_id = db
        .get_or_create_remote_connection(RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.clone().into(),
            port,
            username: user.clone(),
            ..Default::default()
        }))
        .await
        .unwrap();

    assert_eq!(connection_id, same_connection_id);
}

#[gpui::test]
async fn test_get_remote_connections() {
    let db = WorkspaceDb::open_test_db("test_get_remote_connections").await;

    let connections = [
        ("example.com".to_string(), None, None),
        (
            "anotherexample.com".to_string(),
            Some(123_u16),
            Some("user2".to_string()),
        ),
        ("yetanother.com".to_string(), Some(345_u16), None),
    ];

    let mut ids = Vec::new();
    for (host, port, user) in connections.iter() {
        ids.push(
            db.get_or_create_remote_connection(RemoteConnectionOptions::Ssh(
                SshConnectionOptions {
                    host: host.clone().into(),
                    port: *port,
                    username: user.clone(),
                    ..Default::default()
                },
            ))
            .await
            .unwrap(),
        );
    }

    let stored_connections = db.remote_connections().unwrap();
    assert_eq!(
        stored_connections,
        [
            (
                ids[0],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "example.com".into(),
                    port: None,
                    username: None,
                    ..Default::default()
                }),
            ),
            (
                ids[1],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "anotherexample.com".into(),
                    port: Some(123),
                    username: Some("user2".into()),
                    ..Default::default()
                }),
            ),
            (
                ids[2],
                RemoteConnectionOptions::Ssh(SshConnectionOptions {
                    host: "yetanother.com".into(),
                    port: Some(345),
                    username: None,
                    ..Default::default()
                }),
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>(),
    );
}
