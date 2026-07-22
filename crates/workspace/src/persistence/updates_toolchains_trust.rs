use super::*;

impl WorkspaceDb {
    pub(crate) async fn toolchains(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<(Toolchain, Arc<Path>, Arc<RelPath>)>> {
        self.0.write(move |this| {
            let mut select = this
                .select_bound(sql!(
                    SELECT
                        name, path, worktree_root_path, relative_worktree_path, language_name, raw_json
                    FROM toolchains
                    WHERE workspace_id = ?
                ))
                .context("select toolchains")?;

            let toolchain: Vec<(String, String, String, String, String, String)> =
                select(workspace_id)?;

            Ok(toolchain
                .into_iter()
                .filter_map(
                    |(name, path, worktree_root_path, relative_worktree_path, language, json)| {
                        Some((
                            Toolchain {
                                name: name.into(),
                                path: path.into(),
                                language_name: LanguageName::new(&language),
                                as_json: serde_json::Value::from_str(&json).ok()?,
                            },
                           Arc::from(worktree_root_path.as_ref()),
                            RelPath::from_proto(&relative_worktree_path).log_err()?,
                        ))
                    },
                )
                .collect())
        })
        .await
    }

    pub async fn set_toolchain(
        &self,
        workspace_id: WorkspaceId,
        worktree_root_path: Arc<Path>,
        relative_worktree_path: Arc<RelPath>,
        toolchain: Toolchain,
    ) -> Result<()> {
        log::debug!(
            "Setting toolchain for workspace, worktree: {worktree_root_path:?}, relative path: {relative_worktree_path:?}, toolchain: {}",
            toolchain.name
        );
        self.0.write(move |conn| {
            let mut insert = conn
                .exec_bound(sql!(
                    INSERT INTO toolchains(workspace_id, worktree_root_path, relative_worktree_path, language_name, name, path, raw_json) VALUES (?, ?, ?, ?, ?,  ?, ?)
                    ON CONFLICT DO
                    UPDATE SET
                        name = ?5,
                        path = ?6,
                        raw_json = ?7
                ))
                .context("Preparing insertion")?;

            insert((
                workspace_id,
                worktree_root_path.to_string_lossy().into_owned(),
                relative_worktree_path.as_unix_str(),
                toolchain.language_name.as_ref(),
                toolchain.name.as_ref(),
                toolchain.path.as_ref(),
                toolchain.as_json.to_string(),
            ))?;

            Ok(())
        }).await
    }

    pub(crate) async fn save_trusted_worktrees(
        &self,
        trusted_worktrees: HashMap<Option<RemoteHostLocation>, HashSet<PathBuf>>,
    ) -> anyhow::Result<()> {
        use anyhow::Context as _;
        use db::sqlez::statement::Statement;
        use itertools::Itertools as _;

        self.clear_trusted_worktrees()
            .await
            .context("clearing previous trust state")?;

        let trusted_worktrees = trusted_worktrees
            .into_iter()
            .flat_map(|(host, abs_paths)| {
                abs_paths
                    .into_iter()
                    .map(move |abs_path| (Some(abs_path), host.clone()))
            })
            .collect::<Vec<_>>();
        let mut first_worktree;
        let mut last_worktree = 0_usize;
        for (count, placeholders) in std::iter::once("(?, ?, ?)")
            .cycle()
            .take(trusted_worktrees.len())
            .chunks(MAX_QUERY_PLACEHOLDERS / 3)
            .into_iter()
            .map(|chunk| {
                let mut count = 0;
                let placeholders = chunk
                    .inspect(|_| {
                        count += 1;
                    })
                    .join(", ");
                (count, placeholders)
            })
            .collect::<Vec<_>>()
        {
            first_worktree = last_worktree;
            last_worktree = last_worktree + count;
            let query = format!(
                r#"INSERT INTO trusted_worktrees(absolute_path, user_name, host_name)
VALUES {placeholders};"#
            );

            let trusted_worktrees = trusted_worktrees[first_worktree..last_worktree].to_vec();
            self.0
                .write(move |conn| {
                    let mut statement = Statement::prepare(conn, query)?;
                    let mut next_index = 1;
                    for (abs_path, host) in trusted_worktrees {
                        let abs_path = abs_path.as_ref().map(|abs_path| abs_path.to_string_lossy());
                        next_index = statement.bind(
                            &abs_path.as_ref().map(|abs_path| abs_path.as_ref()),
                            next_index,
                        )?;
                        next_index = statement.bind(
                            &host
                                .as_ref()
                                .and_then(|host| Some(host.user_name.as_ref()?.as_str())),
                            next_index,
                        )?;
                        next_index = statement.bind(
                            &host.as_ref().map(|host| host.host_identifier.as_str()),
                            next_index,
                        )?;
                    }
                    statement.exec()
                })
                .await
                .context("inserting new trusted state")?;
        }
        Ok(())
    }

    pub fn fetch_trusted_worktrees(&self) -> Result<DbTrustedPaths> {
        let trusted_worktrees = self.trusted_worktrees()?;
        Ok(trusted_worktrees
            .into_iter()
            .filter_map(|(abs_path, user_name, host_name)| {
                let db_host = match (user_name, host_name) {
                    (None, Some(host_name)) => Some(RemoteHostLocation {
                        user_name: None,
                        host_identifier: SharedString::new(host_name),
                    }),
                    (Some(user_name), Some(host_name)) => Some(RemoteHostLocation {
                        user_name: Some(SharedString::new(user_name)),
                        host_identifier: SharedString::new(host_name),
                    }),
                    _ => None,
                };
                Some((db_host, abs_path?))
            })
            .fold(HashMap::default(), |mut acc, (remote_host, abs_path)| {
                acc.entry(remote_host)
                    .or_insert_with(HashSet::default)
                    .insert(abs_path);
                acc
            }))
    }
}
