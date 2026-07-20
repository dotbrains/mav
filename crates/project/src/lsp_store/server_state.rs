use collections::{BTreeSet, HashMap};
use gpui::Task;
use language::CachedLspAdapter;
use lsp::{LanguageServer, Uri};
use parking_lot::Mutex;
use postage::mpsc;
use std::{
    path::{self, Path, PathBuf},
    sync::Arc,
};

use futures::channel::oneshot;

pub struct WorkspaceRefreshTask {
    pub(super) refresh_tx: mpsc::Sender<Option<oneshot::Sender<bool>>>,
    pub(super) progress_tx: mpsc::Sender<()>,
    #[allow(dead_code)]
    pub(super) task: Task<()>,
}

pub enum LanguageServerState {
    Starting {
        startup: Task<Option<Arc<LanguageServer>>>,
        /// List of language servers that will be added to the workspace once it's initialization completes.
        pending_workspace_folders: Arc<Mutex<BTreeSet<Uri>>>,
    },

    Running {
        adapter: Arc<CachedLspAdapter>,
        server: Arc<LanguageServer>,
        simulate_disk_based_diagnostics_completion: Option<Task<()>>,
        workspace_diagnostics_refresh_tasks: HashMap<Option<String>, WorkspaceRefreshTask>,
    },
}

impl LanguageServerState {
    pub(super) fn add_workspace_folder(&self, uri: Uri) {
        match self {
            LanguageServerState::Starting {
                pending_workspace_folders,
                ..
            } => {
                pending_workspace_folders.lock().insert(uri);
            }
            LanguageServerState::Running { server, .. } => {
                server.add_workspace_folder(uri);
            }
        }
    }

    pub(super) fn _remove_workspace_folder(&self, uri: Uri) {
        match self {
            LanguageServerState::Starting {
                pending_workspace_folders,
                ..
            } => {
                pending_workspace_folders.lock().remove(&uri);
            }
            LanguageServerState::Running { server, .. } => server.remove_workspace_folder(uri),
        }
    }
}

impl std::fmt::Debug for LanguageServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LanguageServerState::Starting { .. } => {
                f.debug_struct("LanguageServerState::Starting").finish()
            }
            LanguageServerState::Running { .. } => {
                f.debug_struct("LanguageServerState::Running").finish()
            }
        }
    }
}

pub(super) fn glob_literal_prefix(glob: &Path) -> PathBuf {
    glob.components()
        .take_while(|component| match component {
            path::Component::Normal(part) => !part.to_string_lossy().contains(['*', '?', '{', '}']),
            _ => true,
        })
        .collect()
}
