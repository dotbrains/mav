use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct RecentWorkspace {
    pub workspace_id: WorkspaceId,
    pub location: SerializedWorkspaceLocation,
    pub paths: PathList,
    pub identity_paths: PathList,
    pub timestamp: DateTime<Utc>,
}

impl RecentWorkspace {
    pub fn project_group_key(&self) -> ProjectGroupKey {
        let host = match &self.location {
            SerializedWorkspaceLocation::Local => None,
            SerializedWorkspaceLocation::Remote(options) => Some(options.clone()),
        };
        ProjectGroupKey::new(host, self.identity_paths.clone())
    }
}

pub(super) async fn resolve_local_workspace_identity(
    fs: &dyn Fs,
    paths: &PathList,
) -> Option<PathList> {
    let raw_paths = paths.paths();
    let resolved_paths = futures::future::join_all(
        raw_paths
            .iter()
            .map(|path| project::git_store::resolve_git_worktree_to_main_repo(fs, path)),
    )
    .await;

    if resolved_paths.iter().all(|resolved| resolved.is_none()) {
        return None;
    }

    let resolved_paths: Vec<PathBuf> = raw_paths
        .iter()
        .zip(resolved_paths.iter())
        .map(|(original, resolved)| {
            resolved
                .as_ref()
                .cloned()
                .unwrap_or_else(|| original.clone())
        })
        .collect();
    let resolved_path_refs: Vec<&Path> = resolved_paths.iter().map(PathBuf::as_path).collect();
    Some(PathList::new(&resolved_path_refs))
}

pub(super) fn dedupe_recent_workspaces(
    workspaces: impl IntoIterator<Item = RecentWorkspace>,
) -> Vec<RecentWorkspace> {
    let mut indices_by_key: HashMap<(Option<RemoteConnectionIdentity>, Vec<PathBuf>), usize> =
        HashMap::default();
    let mut result: Vec<RecentWorkspace> = Vec::new();
    for workspace in workspaces {
        let location_identity = match &workspace.location {
            SerializedWorkspaceLocation::Local => None,
            SerializedWorkspaceLocation::Remote(connection) => {
                Some(remote_connection_identity(connection))
            }
        };
        let key = (location_identity, workspace.identity_paths.paths().to_vec());
        if let Some(&existing_index) = indices_by_key.get(&key) {
            if workspace.timestamp > result[existing_index].timestamp {
                result[existing_index] = workspace;
            }
        } else {
            indices_by_key.insert(key, result.len());
            result.push(workspace);
        }
    }

    result
}
