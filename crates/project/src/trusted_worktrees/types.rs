/// An identifier of a host to split the trust questions by.
/// Each trusted data change and event is done for a particular host.
/// A host may contain more than one worktree or even project open concurrently.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct RemoteHostLocation {
    pub user_name: Option<SharedString>,
    pub host_identifier: SharedString,
}

impl From<RemoteConnectionOptions> for RemoteHostLocation {
    fn from(options: RemoteConnectionOptions) -> Self {
        let (user_name, host_name) = match options {
            RemoteConnectionOptions::Ssh(ssh) => (
                ssh.username.map(SharedString::new),
                SharedString::new(ssh.host.to_string()),
            ),
            RemoteConnectionOptions::Wsl(wsl) => (
                wsl.user.map(SharedString::new),
                SharedString::new(wsl.distro_name),
            ),
            RemoteConnectionOptions::Docker(docker_connection_options) => (
                Some(SharedString::new(docker_connection_options.name)),
                SharedString::new(docker_connection_options.container_id),
            ),
            #[cfg(feature = "test-support")]
            RemoteConnectionOptions::Mock(mock) => {
                (None, SharedString::new(format!("mock-{}", mock.id)))
            }
        };
        Self {
            user_name,
            host_identifier: host_name,
        }
    }
}

/// A unit of trust consideration inside a particular host:
/// either a familiar worktree, or a path that may influence other worktrees' trust.
/// See module-level documentation on the trust model.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum PathTrust {
    /// A worktree that is familiar to this workspace.
    /// Either a single file or a directory worktree.
    Worktree(WorktreeId),
    /// A path that may be another worktree yet not loaded into any workspace (hence, without any `WorktreeId`),
    /// or a parent path coming out of the security modal.
    AbsPath(PathBuf),
}

impl PathTrust {
    pub(super) fn to_proto(&self) -> proto::PathTrust {
        match self {
            Self::Worktree(worktree_id) => proto::PathTrust {
                content: Some(proto::path_trust::Content::WorktreeId(
                    worktree_id.to_proto(),
                )),
            },
            Self::AbsPath(path_buf) => proto::PathTrust {
                content: Some(proto::path_trust::Content::AbsPath(
                    path_buf.to_string_lossy().to_string(),
                )),
            },
        }
    }

    pub fn from_proto(proto: proto::PathTrust) -> Option<Self> {
        Some(match proto.content? {
            proto::path_trust::Content::WorktreeId(id) => {
                Self::Worktree(WorktreeId::from_proto(id))
            }
            proto::path_trust::Content::AbsPath(path) => Self::AbsPath(PathBuf::from(path)),
        })
    }
}

/// A change of trust on a certain host.
#[derive(Debug)]
pub enum TrustedWorktreesEvent {
    Trusted(WeakEntity<WorktreeStore>, HashSet<PathTrust>),
    Restricted(WeakEntity<WorktreeStore>, HashSet<PathTrust>),
}

pub(super) type TrustedPaths = HashMap<WeakEntity<WorktreeStore>, HashSet<PathTrust>>;
pub type DbTrustedPaths = HashMap<Option<RemoteHostLocation>, HashSet<PathBuf>>;
use collections::{HashMap, HashSet};
use gpui::{SharedString, WeakEntity};
use remote::RemoteConnectionOptions;
use rpc::proto;
use settings::WorktreeId;
use std::path::PathBuf;

use crate::worktree_store::WorktreeStore;
