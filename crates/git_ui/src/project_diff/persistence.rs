use anyhow::Context as _;
use db::{
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use project::git_store::branch_diff::DiffBase;
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub struct ProjectDiffDb(ThreadSafeConnection);

impl Domain for ProjectDiffDb {
    const NAME: &str = stringify!(ProjectDiffDb);

    const MIGRATIONS: &[&str] = &[sql!(
            CREATE TABLE project_diffs(
                workspace_id INTEGER,
                item_id INTEGER UNIQUE,

                diff_base TEXT,

                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
    )];
}

db::static_connection!(ProjectDiffDb, [WorkspaceDb]);

impl ProjectDiffDb {
    pub async fn save_diff_base(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
        diff_base: DiffBase,
    ) -> anyhow::Result<()> {
        self.write(move |connection| {
                let sql_stmt = sql!(
                    INSERT OR REPLACE INTO project_diffs(item_id, workspace_id, diff_base) VALUES (?, ?, ?)
                );
                let diff_base_str = serde_json::to_string(&diff_base)?;
                let mut query = connection.exec_bound::<(ItemId, WorkspaceId, String)>(sql_stmt)?;
                query((item_id, workspace_id, diff_base_str)).context(format!(
                    "exec_bound failed to execute or parse for: {}",
                    sql_stmt
                ))
            })
            .await
    }

    pub fn get_diff_base(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<DiffBase> {
        let sql_stmt =
            sql!(SELECT diff_base FROM project_diffs WHERE item_id =  ?AND workspace_id =  ?);
        let diff_base_str = self.select_row_bound::<(ItemId, WorkspaceId), String>(sql_stmt)?((
            item_id,
            workspace_id,
        ))
        .context(::std::format!(
            "Error in get_diff_base, select_row_bound failed to execute or parse for: {}",
            sql_stmt
        ))?;
        let Some(diff_base_str) = diff_base_str else {
            return Ok(DiffBase::Head);
        };
        serde_json::from_str(&diff_base_str).context("deserializing diff base")
    }
}
