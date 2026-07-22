use super::*;

pub struct VimDb(ThreadSafeConnection);

impl Domain for VimDb {
    const NAME: &str = stringify!(VimDb);

    const MIGRATIONS: &[&str] = &[
        sql! (
            CREATE TABLE vim_marks (
              workspace_id INTEGER,
              mark_name TEXT,
              path BLOB,
              value TEXT
            );
            CREATE UNIQUE INDEX idx_vim_marks ON vim_marks (workspace_id, mark_name, path);
        ),
        sql! (
            CREATE TABLE vim_global_marks_paths(
                workspace_id INTEGER,
                mark_name TEXT,
                path BLOB
            );
            CREATE UNIQUE INDEX idx_vim_global_marks_paths
            ON vim_global_marks_paths(workspace_id, mark_name);
        ),
    ];
}

db::static_connection!(VimDb, [WorkspaceDb]);

struct SerializedMark {
    path: Arc<Path>,
    name: String,
    points: Vec<Point>,
}

impl VimDb {
    pub(crate) async fn set_marks(
        &self,
        workspace_id: WorkspaceId,
        path: Arc<Path>,
        marks: HashMap<String, Vec<Point>>,
    ) -> Result<()> {
        log::debug!("Setting path {path:?} for {} marks", marks.len());

        self.write(move |conn| {
            let mut query = conn.exec_bound(sql!(
                INSERT OR REPLACE INTO vim_marks
                    (workspace_id, mark_name, path, value)
                VALUES
                    (?, ?, ?, ?)
            ))?;
            for (mark_name, value) in marks {
                let pairs: Vec<(u32, u32)> = value
                    .into_iter()
                    .map(|point| (point.row, point.column))
                    .collect();
                let serialized = serde_json::to_string(&pairs)?;
                query((workspace_id, mark_name, path.clone(), serialized))?;
            }
            Ok(())
        })
        .await
    }

    fn get_marks(&self, workspace_id: WorkspaceId) -> Result<Vec<SerializedMark>> {
        let result: Vec<(Arc<Path>, String, String)> = self.select_bound(sql!(
            SELECT path, mark_name, value FROM vim_marks
                WHERE workspace_id = ?
        ))?(workspace_id)?;

        Ok(result
            .into_iter()
            .filter_map(|(path, name, value)| {
                let pairs: Vec<(u32, u32)> = serde_json::from_str(&value).log_err()?;
                Some(SerializedMark {
                    path,
                    name,
                    points: pairs
                        .into_iter()
                        .map(|(row, column)| Point { row, column })
                        .collect(),
                })
            })
            .collect())
    }

    pub(crate) async fn delete_mark(
        &self,
        workspace_id: WorkspaceId,
        path: Arc<Path>,
        mark_name: String,
    ) -> Result<()> {
        self.write(move |conn| {
            conn.exec_bound(sql!(
                DELETE FROM vim_marks
                WHERE workspace_id = ? AND mark_name = ? AND path = ?
            ))?((workspace_id, mark_name, path))
        })
        .await
    }

    pub(crate) async fn set_global_mark_path(
        &self,
        workspace_id: WorkspaceId,
        mark_name: String,
        path: Arc<Path>,
    ) -> Result<()> {
        log::debug!("Setting global mark path {path:?} for {mark_name}");
        self.write(move |conn| {
            conn.exec_bound(sql!(
                INSERT OR REPLACE INTO vim_global_marks_paths
                    (workspace_id, mark_name, path)
                VALUES
                    (?, ?, ?)
            ))?((workspace_id, mark_name, path))
        })
        .await
    }

    pub fn get_global_marks_paths(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<(String, Arc<Path>)>> {
        self.select_bound(sql!(
        SELECT mark_name, path FROM vim_global_marks_paths
            WHERE workspace_id = ?
        ))?(workspace_id)
    }

    pub(crate) async fn delete_global_marks_path(
        &self,
        workspace_id: WorkspaceId,
        mark_name: String,
    ) -> Result<()> {
        self.write(move |conn| {
            conn.exec_bound(sql!(
                DELETE FROM vim_global_marks_paths
                WHERE workspace_id = ? AND mark_name = ?
            ))?((workspace_id, mark_name))
        })
        .await
    }
}
