use super::*;

#[derive(Copy, Clone, Default)]
pub struct CreateOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
}

#[derive(Copy, Clone, Default)]
pub struct CopyOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
}

#[derive(Copy, Clone, Default)]
pub struct RenameOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
    /// Whether to create parent directories if they do not exist.
    pub create_parents: bool,
}

#[derive(Copy, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub ignore_if_not_exists: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct Metadata {
    pub inode: u64,
    pub mtime: MTime,
    pub is_symlink: bool,
    pub is_dir: bool,
    pub len: u64,
    pub is_fifo: bool,
    pub is_executable: bool,
}

/// Filesystem modification time. The purpose of this newtype is to discourage use of operations
/// that do not make sense for mtimes. In particular, it is not always valid to compare mtimes using
/// `<` or `>`, as there are many things that can cause the mtime of a file to be earlier than it
/// was. See ["mtime comparison considered harmful" - apenwarr](https://apenwarr.ca/log/20181113).
///
/// Do not derive Ord, PartialOrd, or arithmetic operation traits.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MTime(pub(super) SystemTime);

impl MTime {
    /// Conversion intended for persistence and testing.
    pub fn from_seconds_and_nanos(secs: u64, nanos: u32) -> Self {
        MTime(UNIX_EPOCH + Duration::new(secs, nanos))
    }

    /// Conversion intended for persistence.
    pub fn to_seconds_and_nanos_for_persistence(self) -> Option<(u64, u32)> {
        self.0
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
    }

    /// Returns the value wrapped by this `MTime`, for presentation to the user. The name including
    /// "_for_user" is to discourage misuse - this method should not be used when making decisions
    /// about file dirtiness.
    pub fn timestamp_for_user(self) -> SystemTime {
        self.0
    }

    /// Temporary method to split out the behavior changes from introduction of this newtype.
    pub fn bad_is_greater_than(self, other: MTime) -> bool {
        self.0 > other.0
    }
}

impl From<proto::Timestamp> for MTime {
    fn from(timestamp: proto::Timestamp) -> Self {
        MTime(timestamp.into())
    }
}

impl From<MTime> for proto::Timestamp {
    fn from(mtime: MTime) -> Self {
        mtime.0.into()
    }
}
