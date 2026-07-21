use std::{path::PathBuf, str::FromStr};

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use git::{
    Oid,
    repository::{LogOrder, LogSource, RepoPath},
};
use workspace::WorkspaceDb;

pub struct GitGraphsDb(ThreadSafeConnection);

impl Domain for GitGraphsDb {
    const NAME: &str = stringify!(GitGraphsDb);

    const MIGRATIONS: &[&str] = &[
        sql!(
            CREATE TABLE git_graphs (
                workspace_id INTEGER,
                item_id INTEGER UNIQUE,
                is_open INTEGER DEFAULT FALSE,

                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
        ),
        sql!(
            ALTER TABLE git_graphs ADD COLUMN repo_working_path TEXT;
        ),
        sql!(
            ALTER TABLE git_graphs ADD COLUMN log_source_type TEXT;
            ALTER TABLE git_graphs ADD COLUMN log_source_value TEXT;
            ALTER TABLE git_graphs ADD COLUMN log_order TEXT;
            ALTER TABLE git_graphs ADD COLUMN selected_sha TEXT;
            ALTER TABLE git_graphs ADD COLUMN search_query TEXT;
            ALTER TABLE git_graphs ADD COLUMN search_case_sensitive INTEGER;
        ),
    ];
}

db::static_connection!(GitGraphsDb, [WorkspaceDb]);

pub const LOG_SOURCE_ALL: i32 = 0;
pub const LOG_SOURCE_BRANCH: i32 = 1;
pub const LOG_SOURCE_SHA: i32 = 2;
pub const LOG_SOURCE_PATH: i32 = 3;

pub const LOG_ORDER_DATE: i32 = 0;
pub const LOG_ORDER_TOPO: i32 = 1;
pub const LOG_ORDER_AUTHOR_DATE: i32 = 2;
pub const LOG_ORDER_REVERSE: i32 = 3;

pub fn serialize_log_source_type(log_source: &LogSource) -> i32 {
    match log_source {
        LogSource::All => LOG_SOURCE_ALL,
        LogSource::Branch(_) => LOG_SOURCE_BRANCH,
        LogSource::Sha(_) => LOG_SOURCE_SHA,
        LogSource::Path(_) => LOG_SOURCE_PATH,
    }
}

pub fn serialize_log_source_value(log_source: &LogSource) -> Option<String> {
    match log_source {
        LogSource::All => None,
        LogSource::Branch(branch) => Some(branch.to_string()),
        LogSource::Sha(oid) => Some(oid.to_string()),
        LogSource::Path(path) => Some(path.as_unix_str().to_string()),
    }
}

pub fn serialize_log_order(log_order: &LogOrder) -> i32 {
    match log_order {
        LogOrder::DateOrder => LOG_ORDER_DATE,
        LogOrder::TopoOrder => LOG_ORDER_TOPO,
        LogOrder::AuthorDateOrder => LOG_ORDER_AUTHOR_DATE,
        LogOrder::ReverseChronological => LOG_ORDER_REVERSE,
    }
}

pub fn deserialize_log_source(state: &SerializedGitGraphState) -> LogSource {
    match state.log_source_type {
        Some(LOG_SOURCE_ALL) => LogSource::All,
        Some(LOG_SOURCE_BRANCH) => state
            .log_source_value
            .as_ref()
            .map(|v| LogSource::Branch(v.clone().into()))
            .unwrap_or_default(),
        Some(LOG_SOURCE_SHA) => state
            .log_source_value
            .as_ref()
            .and_then(|v| Oid::from_str(v).ok())
            .map(LogSource::Sha)
            .unwrap_or_default(),
        Some(LOG_SOURCE_PATH) => state
            .log_source_value
            .as_ref()
            .and_then(|v| RepoPath::new(v).ok())
            .map(LogSource::Path)
            .unwrap_or_default(),
        None | Some(_) => LogSource::default(),
    }
}

pub fn deserialize_log_order(state: &SerializedGitGraphState) -> LogOrder {
    match state.log_order {
        Some(LOG_ORDER_DATE) => LogOrder::DateOrder,
        Some(LOG_ORDER_TOPO) => LogOrder::TopoOrder,
        Some(LOG_ORDER_AUTHOR_DATE) => LogOrder::AuthorDateOrder,
        Some(LOG_ORDER_REVERSE) => LogOrder::ReverseChronological,
        _ => LogOrder::default(),
    }
}

#[derive(Debug, Default, Clone)]
pub struct SerializedGitGraphState {
    pub log_source_type: Option<i32>,
    pub log_source_value: Option<String>,
    pub log_order: Option<i32>,
    pub selected_sha: Option<String>,
    pub search_query: Option<String>,
    pub search_case_sensitive: Option<bool>,
}

impl GitGraphsDb {
    query! {
        pub async fn save_git_graph(
            item_id: workspace::ItemId,
            workspace_id: workspace::WorkspaceId,
            repo_working_path: String,
            log_source_type: Option<i32>,
            log_source_value: Option<String>,
            log_order: Option<i32>,
            selected_sha: Option<String>,
            search_query: Option<String>,
            search_case_sensitive: Option<bool>
        ) -> Result<()> {
            INSERT OR REPLACE INTO git_graphs(
                item_id, workspace_id, repo_working_path,
                log_source_type, log_source_value, log_order,
                selected_sha, search_query, search_case_sensitive
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        }
    }

    query! {
        pub fn get_git_graph(
            item_id: workspace::ItemId,
            workspace_id: workspace::WorkspaceId
        ) -> Result<Option<(
            PathBuf,
            Option<i32>,
            Option<String>,
            Option<i32>,
            Option<String>,
            Option<String>,
            Option<bool>
        )>> {
            SELECT
                repo_working_path,
                log_source_type,
                log_source_value,
                log_order,
                selected_sha,
                search_query,
                search_case_sensitive
            FROM git_graphs
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}
