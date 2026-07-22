use super::*;

// ── Migration tests ────────────────────────────────────────────────

#[test]
fn test_thread_id_primary_key_migration_backfills_null_thread_ids() {
    use db::sqlez::connection::Connection;

    let connection = Connection::open_memory(Some("test_thread_id_pk_migration_backfills_nulls"));

    // Run migrations 0-6 (the old schema, before the thread_id PK migration).
    let old_migrations: &[&str] = &ThreadMetadataDb::MIGRATIONS[..7];
    connection
        .migrate(ThreadMetadataDb::NAME, old_migrations, &mut |_, _, _| false)
        .expect("old migrations should succeed");

    // Insert rows: one with a thread_id, two without.
    connection
        .exec(
            "INSERT INTO sidebar_threads \
             (session_id, title, updated_at, thread_id) \
             VALUES ('has-tid', 'Has ThreadId', '2025-01-01T00:00:00Z', X'0102030405060708090A0B0C0D0E0F10')",
        )
        .unwrap()()
        .unwrap();
    connection
        .exec(
            "INSERT INTO sidebar_threads \
             (session_id, title, updated_at) \
             VALUES ('no-tid-1', 'No ThreadId 1', '2025-01-02T00:00:00Z')",
        )
        .unwrap()()
    .unwrap();
    connection
        .exec(
            "INSERT INTO sidebar_threads \
             (session_id, title, updated_at) \
             VALUES ('no-tid-2', 'No ThreadId 2', '2025-01-03T00:00:00Z')",
        )
        .unwrap()()
    .unwrap();

    // Set up archived_git_worktrees + thread_archived_worktrees rows
    // referencing the session without a thread_id.
    connection
        .exec(
            "INSERT INTO archived_git_worktrees \
             (id, worktree_path, main_repo_path, staged_commit_hash, unstaged_commit_hash, original_commit_hash) \
             VALUES (1, '/wt', '/main', 'abc', 'def', '000')",
        )
        .unwrap()()
        .unwrap();
    connection
        .exec(
            "INSERT INTO thread_archived_worktrees \
             (session_id, archived_worktree_id) \
             VALUES ('no-tid-1', 1)",
        )
        .unwrap()()
    .unwrap();

    // Run all current migrations. sqlez skips the already-applied ones and
    // runs the remaining migrations.
    run_thread_metadata_migrations(&connection);

    // All 3 rows should survive with non-NULL thread_ids.
    let count: i64 = connection
        .select_row_bound::<(), i64>("SELECT COUNT(*) FROM sidebar_threads")
        .unwrap()(())
    .unwrap()
    .unwrap();
    assert_eq!(count, 3, "all 3 rows should survive the migration");

    let null_count: i64 = connection
        .select_row_bound::<(), i64>("SELECT COUNT(*) FROM sidebar_threads WHERE thread_id IS NULL")
        .unwrap()(())
    .unwrap()
    .unwrap();
    assert_eq!(
        null_count, 0,
        "no rows should have NULL thread_id after migration"
    );

    // The row that already had a thread_id should keep its original value.
    let original_tid: Vec<u8> = connection
        .select_row_bound::<&str, Vec<u8>>(
            "SELECT thread_id FROM sidebar_threads WHERE session_id = ?",
        )
        .unwrap()("has-tid")
    .unwrap()
    .unwrap();
    assert_eq!(
        original_tid,
        vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10
        ],
        "pre-existing thread_id should be preserved"
    );

    // The two rows that had NULL thread_ids should now have distinct non-empty blobs.
    let generated_tid_1: Vec<u8> = connection
        .select_row_bound::<&str, Vec<u8>>(
            "SELECT thread_id FROM sidebar_threads WHERE session_id = ?",
        )
        .unwrap()("no-tid-1")
    .unwrap()
    .unwrap();
    let generated_tid_2: Vec<u8> = connection
        .select_row_bound::<&str, Vec<u8>>(
            "SELECT thread_id FROM sidebar_threads WHERE session_id = ?",
        )
        .unwrap()("no-tid-2")
    .unwrap()
    .unwrap();
    assert_eq!(
        generated_tid_1.len(),
        16,
        "generated thread_id should be 16 bytes"
    );
    assert_eq!(
        generated_tid_2.len(),
        16,
        "generated thread_id should be 16 bytes"
    );
    assert_ne!(
        generated_tid_1, generated_tid_2,
        "each generated thread_id should be unique"
    );

    // The thread_archived_worktrees join row should have migrated
    // using the backfilled thread_id from the session without a
    // pre-existing thread_id.
    let archived_count: i64 = connection
        .select_row_bound::<(), i64>("SELECT COUNT(*) FROM thread_archived_worktrees")
        .unwrap()(())
    .unwrap()
    .unwrap();
    assert_eq!(
        archived_count, 1,
        "thread_archived_worktrees row should survive migration"
    );

    // The thread_archived_worktrees row should reference the
    // backfilled thread_id of the 'no-tid-1' session.
    let archived_tid: Vec<u8> = connection
        .select_row_bound::<(), Vec<u8>>("SELECT thread_id FROM thread_archived_worktrees LIMIT 1")
        .unwrap()(())
    .unwrap()
    .unwrap();
    assert_eq!(
        archived_tid, generated_tid_1,
        "thread_archived_worktrees should reference the backfilled thread_id"
    );
}
