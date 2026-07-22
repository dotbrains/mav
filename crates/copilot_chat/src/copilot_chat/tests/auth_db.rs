use super::super::auth::extract_oauth_token_from_db;
use super::*;

#[test]
fn test_extract_oauth_token_from_db_matches_auth_authority_and_recency() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("auth.db");
    let older_github_token = "ghu_oldergithubtokenvalue000000000000";
    let newer_github_token = "ghu_newergithubtokenvalue000000000000";
    let enterprise_token = "ghu_enterprisetokenvalue0000000000000";

    let connection = sqlez::connection::Connection::open_file(db_path.to_str().unwrap());
    connection
        .exec(
            "CREATE TABLE oauth_tokens (
                token_id INTEGER PRIMARY KEY AUTOINCREMENT,
                auth_authority TEXT NOT NULL,
                token_ciphertext BLOB NOT NULL,
                last_used_at INTEGER NOT NULL
            );",
        )
        .unwrap()()
    .unwrap();

    {
        let mut insert_token = connection
            .exec_bound::<(&str, Vec<u8>, i64)>(
                "INSERT INTO oauth_tokens (auth_authority, token_ciphertext, last_used_at) VALUES (?, ?, ?);",
            )
            .unwrap();
        insert_token(("github.com", older_github_token.as_bytes().to_vec(), 10)).unwrap();
        insert_token((
            "github.enterprise.test",
            enterprise_token.as_bytes().to_vec(),
            30,
        ))
        .unwrap();
        insert_token(("github.com", newer_github_token.as_bytes().to_vec(), 20)).unwrap();
    }
    drop(connection);

    assert_eq!(
        extract_oauth_token_from_db(&db_path, "github.com").as_deref(),
        Some(newer_github_token)
    );
    assert_eq!(
        extract_oauth_token_from_db(&db_path, "github.enterprise.test").as_deref(),
        Some(enterprise_token)
    );
}

#[test]
fn test_extract_oauth_token_from_db_missing_db_does_not_create_file() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("auth.db");

    assert_eq!(extract_oauth_token_from_db(&db_path, "github.com"), None);
    assert!(!db_path.exists());
}
