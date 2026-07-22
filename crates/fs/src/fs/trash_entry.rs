use super::*;

// We use our own type rather than `trash::TrashItem` directly to avoid carrying
// over fields we don't need (e.g. `time_deleted`) and to insulate callers and
// tests from changes to that crate's API surface.
/// Represents a file or directory that has been moved to the system trash,
/// retaining enough information to restore it to its original location.
#[derive(Clone, PartialEq, Debug)]
pub struct TrashedEntry {
    /// Platform-specific identifier for the file/directory in the trash.
    ///
    /// * Freedesktop – Path to the `.trashinfo` file.
    /// * macOS & Windows – Full path to the file/directory in the system's
    /// trash.
    pub id: OsString,
    /// Name of the file/directory at the time of trashing, including extension.
    pub name: OsString,
    /// Absolute path to the parent directory at the time of trashing.
    pub original_parent: PathBuf,
}

impl From<trash::TrashItem> for TrashedEntry {
    fn from(item: trash::TrashItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            original_parent: item.original_parent,
        }
    }
}

impl TrashedEntry {
    pub(super) fn into_trash_item(self) -> trash::TrashItem {
        trash::TrashItem {
            id: self.id,
            name: self.name,
            original_parent: self.original_parent,
            // `TrashedEntry` doesn't preserve `time_deleted` as we don't
            // currently need it for restore, so we default it to 0 here.
            time_deleted: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TrashRestoreError {
    #[error("The specified `path` ({}) was not found in the system's trash.", path.display())]
    NotFound { path: PathBuf },
    #[error("File or directory ({}) already exists at the restore destination.", path.display())]
    Collision { path: PathBuf },
    #[error("Unknown error ({description})")]
    Unknown { description: String },
}

impl From<trash::Error> for TrashRestoreError {
    fn from(err: trash::Error) -> Self {
        match err {
            trash::Error::RestoreCollision { path, .. } => Self::Collision { path },
            trash::Error::Unknown { description } => Self::Unknown { description },
            other => Self::Unknown {
                description: other.to_string(),
            },
        }
    }
}
