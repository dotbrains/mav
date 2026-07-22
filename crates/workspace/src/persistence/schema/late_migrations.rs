use super::*;

pub(super) const TOOLCHAINS_ROOT_PATHS: &str = sql!(CREATE TABLE toolchains2 (
    workspace_id INTEGER,
    worktree_root_path TEXT NOT NULL,
    language_name TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    relative_worktree_path TEXT NOT NULL,
    PRIMARY KEY (workspace_id, worktree_root_path, language_name, relative_worktree_path)) STRICT;
    INSERT OR REPLACE INTO toolchains2
        // The `instr(paths, '\n') = 0` part allows us to find all
        // workspaces that have a single worktree, as `\n` is used as a
        // separator when serializing the workspace paths, so if no `\n` is
        // found, we know we have a single worktree.
        SELECT toolchains.workspace_id, paths, language_name, name, path, raw_json, relative_worktree_path FROM toolchains INNER JOIN workspaces ON toolchains.workspace_id = workspaces.workspace_id AND instr(paths, '\n') = 0;
    DROP TABLE toolchains;
    ALTER TABLE toolchains2 RENAME TO toolchains;
);

pub(super) const USER_TOOLCHAINS_ROOT_PATHS: &str = sql!(CREATE TABLE user_toolchains2 (
    remote_connection_id INTEGER,
    workspace_id INTEGER NOT NULL,
    worktree_root_path TEXT NOT NULL,
    relative_worktree_path TEXT NOT NULL,
    language_name TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    raw_json TEXT NOT NULL,

    PRIMARY KEY (workspace_id, worktree_root_path, relative_worktree_path, language_name, name, path, raw_json)) STRICT;
    INSERT OR REPLACE INTO user_toolchains2
        // The `instr(paths, '\n') = 0` part allows us to find all
        // workspaces that have a single worktree, as `\n` is used as a
        // separator when serializing the workspace paths, so if no `\n` is
        // found, we know we have a single worktree.
        SELECT user_toolchains.remote_connection_id, user_toolchains.workspace_id, paths, relative_worktree_path, language_name, name, path, raw_json  FROM user_toolchains INNER JOIN workspaces ON user_toolchains.workspace_id = workspaces.workspace_id AND instr(paths, '\n') = 0;
    DROP TABLE user_toolchains;
    ALTER TABLE user_toolchains2 RENAME TO user_toolchains;
);
