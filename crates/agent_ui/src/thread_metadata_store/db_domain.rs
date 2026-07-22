use super::*;

struct ThreadMetadataDb(ThreadSafeConnection);

impl Domain for ThreadMetadataDb {
    const NAME: &str = stringify!(ThreadMetadataDb);

    const MIGRATIONS: &[&str] = &[
        sql!(
            CREATE TABLE IF NOT EXISTS sidebar_threads(
                session_id TEXT PRIMARY KEY,
                agent_id TEXT,
                title TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                created_at TEXT,
                folder_paths TEXT,
                folder_paths_order TEXT
            ) STRICT;
        ),
        sql!(ALTER TABLE sidebar_threads ADD COLUMN archived INTEGER DEFAULT 0),
        sql!(ALTER TABLE sidebar_threads ADD COLUMN main_worktree_paths TEXT),
        sql!(ALTER TABLE sidebar_threads ADD COLUMN main_worktree_paths_order TEXT),
        sql!(
            CREATE TABLE IF NOT EXISTS archived_git_worktrees(
                id INTEGER PRIMARY KEY,
                worktree_path TEXT NOT NULL,
                main_repo_path TEXT NOT NULL,
                branch_name TEXT,
                staged_commit_hash TEXT,
                unstaged_commit_hash TEXT,
                original_commit_hash TEXT
            ) STRICT;

            CREATE TABLE IF NOT EXISTS thread_archived_worktrees(
                session_id TEXT NOT NULL,
                archived_worktree_id INTEGER NOT NULL REFERENCES archived_git_worktrees(id),
                PRIMARY KEY (session_id, archived_worktree_id)
            ) STRICT;
        ),
        sql!(ALTER TABLE sidebar_threads ADD COLUMN remote_connection TEXT),
        sql!(ALTER TABLE sidebar_threads ADD COLUMN thread_id BLOB),
        sql!(
            UPDATE sidebar_threads SET thread_id = randomblob(16) WHERE thread_id IS NULL;

            CREATE TABLE thread_archived_worktrees_v2(
                thread_id BLOB NOT NULL,
                archived_worktree_id INTEGER NOT NULL REFERENCES archived_git_worktrees(id),
                PRIMARY KEY (thread_id, archived_worktree_id)
            ) STRICT;

            INSERT INTO thread_archived_worktrees_v2(thread_id, archived_worktree_id)
            SELECT s.thread_id, t.archived_worktree_id
            FROM thread_archived_worktrees t
            JOIN sidebar_threads s ON s.session_id = t.session_id;

            DROP TABLE thread_archived_worktrees;
            ALTER TABLE thread_archived_worktrees_v2 RENAME TO thread_archived_worktrees;

            CREATE TABLE sidebar_threads_v2(
                thread_id BLOB PRIMARY KEY,
                session_id TEXT,
                agent_id TEXT,
                title TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                created_at TEXT,
                folder_paths TEXT,
                folder_paths_order TEXT,
                archived INTEGER DEFAULT 0,
                main_worktree_paths TEXT,
                main_worktree_paths_order TEXT,
                remote_connection TEXT
            ) STRICT;

            INSERT INTO sidebar_threads_v2(thread_id, session_id, agent_id, title, updated_at, created_at, folder_paths, folder_paths_order, archived, main_worktree_paths, main_worktree_paths_order, remote_connection)
            SELECT thread_id, session_id, agent_id, title, updated_at, created_at, folder_paths, folder_paths_order, archived, main_worktree_paths, main_worktree_paths_order, remote_connection
            FROM sidebar_threads;

            DROP TABLE sidebar_threads;
            ALTER TABLE sidebar_threads_v2 RENAME TO sidebar_threads;
        ),
        sql!(
            DELETE FROM thread_archived_worktrees
            WHERE thread_id IN (
                SELECT thread_id FROM sidebar_threads WHERE session_id IS NULL
            );

            DELETE FROM sidebar_threads WHERE session_id IS NULL;

            DELETE FROM archived_git_worktrees
            WHERE id NOT IN (
                SELECT archived_worktree_id FROM thread_archived_worktrees
            );
        ),
        sql!(
            ALTER TABLE sidebar_threads ADD COLUMN interacted_at TEXT;
        ),
        sql!(
            ALTER TABLE sidebar_threads ADD COLUMN title_override TEXT;
        ),
    ];
}

db::static_connection!(ThreadMetadataDb, []);
