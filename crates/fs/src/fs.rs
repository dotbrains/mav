#[path = "fs/copy_recursive.rs"]
mod copy_recursive;
#[path = "fs/fake_file_methods.rs"]
#[cfg(feature = "test-support")]
mod fake_file_methods;
#[path = "fs/fake_git_state_helpers.rs"]
#[cfg(feature = "test-support")]
mod fake_git_state_helpers;
#[path = "fs/fake_handles.rs"]
#[cfg(feature = "test-support")]
mod fake_handles;
#[path = "fs/fake_helpers.rs"]
#[cfg(feature = "test-support")]
mod fake_helpers;
#[path = "fs/fake_impl.rs"]
#[cfg(feature = "test-support")]
mod fake_impl;
#[path = "fs/fake_query_helpers.rs"]
#[cfg(feature = "test-support")]
mod fake_query_helpers;
#[path = "fs/fake_tree_helpers.rs"]
#[cfg(feature = "test-support")]
mod fake_tree_helpers;
#[path = "fs/fake_types.rs"]
#[cfg(feature = "test-support")]
mod fake_types;
#[path = "fs/fake_watch_git_methods.rs"]
#[cfg(feature = "test-support")]
mod fake_watch_git_methods;
pub mod fs_watcher;
#[path = "fs/job.rs"]
mod job;
#[path = "fs/metadata.rs"]
mod metadata;
#[path = "fs/real.rs"]
mod real;
#[path = "fs/real_file_methods.rs"]
mod real_file_methods;
#[path = "fs/real_git_methods.rs"]
mod real_git_methods;
#[path = "fs/trash_entry.rs"]
mod trash_entry;
#[path = "fs/windows.rs"]
#[cfg(target_os = "windows")]
mod windows;

pub use fs_watcher::requires_poll_watcher;
use job::JobTracker;
pub use job::{JobEvent, JobEventReceiver, JobEventSender, JobId, JobInfo};
pub use metadata::{CopyOptions, CreateOptions, MTime, Metadata, RemoveOptions, RenameOptions};
pub use trash_entry::{TrashRestoreError, TrashedEntry};

pub use copy_recursive::{copy_recursive, read_dir_items};
#[cfg(feature = "test-support")]
use fake_handles::{FakeHandle, FakeWatcher};
#[cfg(feature = "test-support")]
pub(crate) use fake_types::FakeFsEntry;
#[cfg(feature = "test-support")]
use fake_types::FakeFsState;
#[cfg(feature = "test-support")]
pub use fake_types::{FS_DOT_GIT, FakeFs};
use parking_lot::Mutex;
pub use real::{FileHandle, RealFs};
use std::ffi::OsString;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Instant;
use util::maybe;

use anyhow::{Context as _, Result, anyhow};
use futures::stream::iter;
use gpui::App;
use gpui::BackgroundExecutor;
use gpui::Global;
use gpui::ReadGlobal as _;
use gpui::SharedString;
#[cfg(unix)]
use std::ffi::CString;
use util::command::new_command;

#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt};

#[cfg(any(target_os = "macos", target_os = "freebsd"))]
use std::mem::MaybeUninit;

use async_tar::Archive;
use futures::{AsyncRead, Stream, StreamExt, future::BoxFuture};
use git::repository::{GitRepository, RealGitRepository};
use is_executable::IsExecutable;
use rope::Rope;
use serde::{Deserialize, Serialize};
use smol::io::AsyncWriteExt;
#[cfg(feature = "test-support")]
use std::path::Component;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use text::LineEnding;

#[cfg(feature = "test-support")]
mod fake_git_repo;
#[cfg(feature = "test-support")]
use collections::{BTreeMap, btree_map};
#[cfg(feature = "test-support")]
use fake_git_repo::{FakeCommitDataEntry, FakeGitRepositoryState};
#[cfg(feature = "test-support")]
use git::{
    repository::{CommitData, InitialGraphCommitData, RepoPath, Worktree, repo_path},
    status::{FileStatus, StatusCode, TrackedStatus, UnmergedStatus},
};
#[cfg(feature = "test-support")]
use util::normalize_path;

#[cfg(feature = "test-support")]
use smol::io::AsyncReadExt;
#[cfg(feature = "test-support")]
use std::ffi::OsStr;

pub trait Watcher: Send + Sync {
    fn add(&self, path: &Path) -> Result<()>;
    fn remove(&self, path: &Path) -> Result<()>;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum PathEventKind {
    Removed,
    Created,
    Changed,
    Rescan,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct PathEvent {
    pub path: PathBuf,
    pub kind: Option<PathEventKind>,
}

impl From<PathEvent> for PathBuf {
    fn from(event: PathEvent) -> Self {
        event.path
    }
}

#[async_trait::async_trait]
pub trait Fs: Send + Sync {
    async fn create_dir(&self, path: &Path) -> Result<()>;
    async fn create_symlink(&self, path: &Path, target: PathBuf) -> Result<()>;
    async fn create_file(&self, path: &Path, options: CreateOptions) -> Result<()>;
    async fn create_file_with(
        &self,
        path: &Path,
        content: Pin<&mut (dyn AsyncRead + Send)>,
    ) -> Result<()>;
    async fn extract_tar_file(
        &self,
        path: &Path,
        content: Archive<Pin<&mut (dyn AsyncRead + Send)>>,
    ) -> Result<()>;
    async fn copy_file(&self, source: &Path, target: &Path, options: CopyOptions) -> Result<()>;
    async fn rename(&self, source: &Path, target: &Path, options: RenameOptions) -> Result<()>;

    /// Removes a directory from the filesystem.
    /// There is no expectation that the directory will be preserved in the
    /// system trash.
    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> Result<()>;

    /// Moves a file or directory to the system trash.
    /// Returns a [`TrashedEntry`] that can be used to keep track of the
    /// location of the trashed item in the system's trash.
    async fn trash(&self, path: &Path, options: RemoveOptions) -> Result<TrashedEntry>;

    /// Removes a file from the filesystem.
    /// There is no expectation that the file will be preserved in the system
    /// trash.
    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> Result<()>;

    async fn open_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>>;
    async fn open_sync(&self, path: &Path) -> Result<Box<dyn io::Read + Send + Sync>>;
    async fn load(&self, path: &Path) -> Result<String> {
        Ok(String::from_utf8(self.load_bytes(path).await?)?)
    }
    async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>>;
    async fn atomic_write(&self, path: PathBuf, text: String) -> Result<()>;
    async fn save(&self, path: &Path, text: &Rope, line_ending: LineEnding) -> Result<()>;
    async fn write(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf>;
    async fn is_file(&self, path: &Path) -> bool;
    async fn is_dir(&self, path: &Path) -> bool;
    async fn metadata(&self, path: &Path) -> Result<Option<Metadata>>;
    async fn read_link(&self, path: &Path) -> Result<PathBuf>;
    async fn read_dir(
        &self,
        path: &Path,
    ) -> Result<Pin<Box<dyn Send + Stream<Item = Result<PathBuf>>>>>;

    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    );

    fn open_repo(
        &self,
        abs_dot_git: &Path,
        system_git_binary_path: Option<&Path>,
    ) -> Result<Arc<dyn GitRepository>>;
    async fn git_init(&self, abs_work_directory: &Path, fallback_branch_name: String)
    -> Result<()>;
    async fn git_clone(&self, abs_work_directory: &Path, repo_url: &str) -> Result<()>;
    async fn git_config(&self, abs_work_directory: &Path, args: Vec<String>) -> Result<String>;
    fn is_fake(&self) -> bool;
    async fn is_case_sensitive(&self) -> bool;
    fn subscribe_to_jobs(&self) -> JobEventReceiver;

    /// Restores a given `TrashedEntry`, moving it from the system's trash back
    /// to the original path.
    async fn restore(
        &self,
        trashed_entry: TrashedEntry,
    ) -> std::result::Result<PathBuf, TrashRestoreError>;

    #[cfg(feature = "test-support")]
    fn as_fake(&self) -> Arc<FakeFs> {
        panic!("called as_fake on a real fs");
    }
}

struct GlobalFs(Arc<dyn Fs>);

impl Global for GlobalFs {}

impl dyn Fs {
    /// Returns the global [`Fs`].
    pub fn global(cx: &App) -> Arc<Self> {
        GlobalFs::global(cx).0.clone()
    }

    /// Sets the global [`Fs`].
    pub fn set_global(fs: Arc<Self>, cx: &mut App) {
        cx.set_global(GlobalFs(fs));
    }
}
