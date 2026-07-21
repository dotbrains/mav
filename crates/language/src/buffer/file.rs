use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferEditSource {
    User,
    Agent,
    Remote,
}

impl BufferEditSource {
    pub fn is_local(self) -> bool {
        !matches!(self, Self::Remote)
    }
}

/// The file associated with a buffer.
pub trait File: Send + Sync + Any {
    /// Returns the [`LocalFile`] associated with this file, if the
    /// file is local.
    fn as_local(&self) -> Option<&dyn LocalFile>;

    /// Returns whether this file is local.
    fn is_local(&self) -> bool {
        self.as_local().is_some()
    }

    /// Returns whether the file is new, exists in storage, or has been deleted. Includes metadata
    /// only available in some states, such as modification time.
    fn disk_state(&self) -> DiskState;

    /// Returns the path of this file relative to the worktree's root directory.
    fn path(&self) -> &Arc<RelPath>;

    /// Returns the path of this file relative to the worktree's parent directory (this means it
    /// includes the name of the worktree's root folder).
    fn full_path(&self, cx: &App) -> PathBuf;

    /// Returns the path style of this file.
    fn path_style(&self, cx: &App) -> PathStyle;

    /// Returns the last component of this handle's absolute path. If this handle refers to the root
    /// of its worktree, then this method will return the name of the worktree itself.
    fn file_name<'a>(&'a self, cx: &'a App) -> &'a str;

    /// Returns the id of the worktree to which this file belongs.
    ///
    /// This is needed for looking up project-specific settings.
    fn worktree_id(&self, cx: &App) -> WorktreeId;

    /// Converts this file into a protobuf message.
    fn to_proto(&self, cx: &App) -> rpc::proto::File;

    /// Return whether Mav considers this to be a private file.
    fn is_private(&self) -> bool;

    fn can_open(&self) -> bool {
        !self.is_local()
    }
}

/// The file's storage status - whether it's stored (`Present`), and if so when it was last
/// modified. In the case where the file is not stored, it can be either `New` or `Deleted`. In the
/// UI these two states are distinguished. For example, the buffer tab does not display a deletion
/// indicator for new files.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DiskState {
    /// File created in Mav that has not been saved.
    New,
    /// File present on the filesystem.
    Present { mtime: MTime, size: u64 },
    /// Deleted file that was previously present.
    Deleted,
    /// An old version of a file that was previously present
    /// usually from a version control system. e.g. A git blob
    Historic { was_deleted: bool },
}

impl DiskState {
    /// Returns the file's last known modification time on disk.
    pub fn mtime(self) -> Option<MTime> {
        match self {
            DiskState::New => None,
            DiskState::Present { mtime, .. } => Some(mtime),
            DiskState::Deleted => None,
            DiskState::Historic { .. } => None,
        }
    }

    /// Returns the file's size on disk in bytes.
    pub fn size(self) -> Option<u64> {
        match self {
            DiskState::New => None,
            DiskState::Present { size, .. } => Some(size),
            DiskState::Deleted => None,
            DiskState::Historic { .. } => None,
        }
    }

    pub fn exists(&self) -> bool {
        match self {
            DiskState::New => false,
            DiskState::Present { .. } => true,
            DiskState::Deleted => false,
            DiskState::Historic { .. } => false,
        }
    }

    /// Returns true if this state represents a deleted file.
    pub fn is_deleted(&self) -> bool {
        match self {
            DiskState::Deleted => true,
            DiskState::Historic { was_deleted } => *was_deleted,
            _ => false,
        }
    }
}

/// The file associated with a buffer, in the case where the file is on the local disk.
pub trait LocalFile: File {
    /// Returns the absolute path of this file
    fn abs_path(&self, cx: &App) -> PathBuf;

    /// Loads the file contents from disk and returns them as a UTF-8 encoded string.
    fn load(&self, cx: &App) -> Task<Result<String>>;

    /// Loads the file's contents from disk.
    fn load_bytes(&self, cx: &App) -> Task<Result<Vec<u8>>>;
}
