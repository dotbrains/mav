mod background_scanner;
mod ignore;
mod worktree_file;
mod worktree_repository;
mod worktree_scan_state;
mod worktree_settings;
mod worktree_state;
mod worktree_traits;
mod worktree_types;

use ::ignore::gitignore::{Gitignore, GitignoreBuilder};
use anyhow::{Context as _, Result, anyhow};
use chardetng::EncodingDetector;
use clock::ReplicaId;
use collections::{BTreeMap, HashMap, HashSet, VecDeque};
use encoding_rs::Encoding;
use fs::{
    Fs, MTime, PathEvent, PathEventKind, RemoveOptions, TrashedEntry, Watcher, copy_recursive,
    read_dir_items,
};
use futures::{
    FutureExt as _, Stream, StreamExt,
    channel::{
        mpsc::{self, UnboundedSender},
        oneshot,
    },
    select_biased, stream,
    task::Poll,
};
use fuzzy::CharBag;
use git::{
    BISECT_LOG, COMMIT_MESSAGE, DOT_GIT, FETCH_HEAD, FSMONITOR_DAEMON, GC_PID, GITIGNORE,
    HOOKS_DIR, INFO_DIR, LFS_DIR, LOGS_DIR, LOGS_REF_STASH, OBJECTS_DIR, ORIG_HEAD,
    REBASE_APPLY_DIR, REBASE_MERGE_DIR, REPO_EXCLUDE, SEQUENCER_DIR, status::GitSummary,
};
use gpui::{
    App, AppContext as _, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, Priority,
    Task,
};
use ignore::IgnoreStack;
use language::{ByteContent, DiskState, FILE_ANALYSIS_BYTES, analyze_byte_content};

use async_channel::{self, Sender};
use background_scanner::{BackgroundScanner, BackgroundScannerPhase};
use parking_lot::Mutex;
use paths::{local_settings_folder_name, local_vscode_folder_name};
use postage::{
    barrier,
    prelude::{Sink as _, Stream as _},
    watch,
};
use rpc::{
    AnyProtoClient,
    proto::{self, split_worktree_update},
};
pub use settings::WorktreeId;
use settings::{Settings, SettingsLocation, SettingsStore};
use smallvec::{SmallVec, smallvec};
use std::{
    any::Any,
    borrow::Borrow as _,
    cmp::Ordering,
    collections::hash_map,
    convert::TryFrom,
    ffi::OsStr,
    fmt,
    future::Future,
    mem::{self},
    ops::{Deref, DerefMut, Range},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use sum_tree::{Bias, Dimensions, Edit, KeyedItem, SeekTarget, SumTree, Summary, TreeMap, TreeSet};
use text::{LineEnding, Rope};
use util::{
    ResultExt, maybe,
    paths::{PathMatcher, PathStyle, SanitizedPath, home_dir},
    rel_path::{RelPath, RelPathBuf},
};
use worktree_repository::LocalRepositoryEntry;
use worktree_scan_state::{
    BackgroundScannerState, EventRoot, PathPrefixScanRequest, ScanRequest, ScanState,
    UpdateObservationState,
};
pub use worktree_settings::WorktreeSettings;
pub use worktree_state::{LocalSnapshot, LocalWorktree, RemoteWorktree, Snapshot};
pub use worktree_types::{
    CreatedEntry, Event, LoadedBinaryFile, LoadedFile, ProjectEntryId, WorkDirectory,
};

use crate::ignore::IgnoreKind;
pub use worktree_file::File;

pub const FS_WATCH_LATENCY: Duration = Duration::from_millis(100);

/// A set of local or remote files that are being opened as part of a project.
/// Responsible for tracking related FS (for local)/collab (for remote) events and corresponding updates.
/// Stores git repositories data and the diagnostics for the file(s).
///
/// Has an absolute path, and may be set to be visible in Mav UI or not.
/// May correspond to a directory or a single file.
/// Possible examples:
/// * a drag and dropped file — may be added as an invisible, "ephemeral" entry to the current worktree
/// * a directory opened in Mav — may be added as a visible entry to the current worktree
///
/// Uses [`Entry`] to track the state of each file/directory, can look up absolute paths for entries.
pub enum Worktree {
    Local(LocalWorktree),
    Remote(RemoteWorktree),
}

impl Deref for LocalRepositoryEntry {
    type Target = WorkDirectory;

    fn deref(&self) -> &Self::Target {
        &self.work_directory
    }
}

impl Deref for LocalSnapshot {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl DerefMut for LocalSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.snapshot
    }
}

impl EventEmitter<Event> for Worktree {}

#[path = "worktree/background_scanner_state_impl.rs"]
mod background_scanner_state_impl;
#[path = "worktree/local_snapshot_impl.rs"]
mod local_snapshot_impl;
#[path = "worktree/local_worktree_impl.rs"]
mod local_worktree_impl;
#[path = "worktree/local_worktree_io.rs"]
mod local_worktree_io;
#[path = "worktree/local_worktree_mutation.rs"]
mod local_worktree_mutation;
#[path = "worktree/local_worktree_refresh.rs"]
mod local_worktree_refresh;
#[path = "worktree/local_worktree_scanner.rs"]
mod local_worktree_scanner;
#[path = "worktree/remote_worktree_impl.rs"]
mod remote_worktree_impl;
#[path = "worktree/snapshot_impl.rs"]
mod snapshot_impl;
#[path = "worktree/worktree_accessors.rs"]
mod worktree_accessors;
#[path = "worktree/worktree_impl.rs"]
mod worktree_impl;
#[path = "worktree/worktree_operations.rs"]
mod worktree_operations;
#[path = "worktree/worktree_proto_handlers.rs"]
mod worktree_proto_handlers;

async fn is_dot_git(path: &Path, fs: &dyn Fs) -> bool {
    if let Some(file_name) = path.file_name()
        && file_name == DOT_GIT
    {
        return true;
    }

    // If we're in a bare repository, we are not inside a `.git` folder. In a
    // bare repository, the root folder contains what would normally be in the
    // `.git` folder.
    let head_metadata = fs.metadata(&path.join("HEAD")).await;
    if !matches!(head_metadata, Ok(Some(_))) {
        return false;
    }
    let config_metadata = fs.metadata(&path.join("config")).await;
    matches!(config_metadata, Ok(Some(_)))
}

mod worktree_diff;
use worktree_diff::{build_diff, char_bag_for_path, merge_event_roots, swap_to_front};

mod worktree_entry;
use worktree_entry::PathEntry;
pub use worktree_entry::{
    Entry, EntryKind, EntrySummary, PathChange, PathKey, PathProgress, PathSummary,
    UpdatedEntriesSet, UpdatedGitRepositoriesSet, UpdatedGitRepository,
};

mod worktree_file_decoding;
use worktree_file_decoding::decode_file_text;

mod worktree_git_discovery;
pub use worktree_git_discovery::discover_root_repo_common_dir;
use worktree_git_discovery::{
    NullWatcher, build_gitignore, build_gitignore_with_root, discover_ancestor_git_repo,
    discover_git_paths,
};

mod worktree_model_handle;
pub use worktree_model_handle::WorktreeModelHandle;

mod worktree_scan_jobs;
use worktree_scan_jobs::{ScanJob, UpdateIgnoreStatusJob};

mod worktree_traversal;
pub use worktree_traversal::{ChildEntriesIter, ChildEntriesOptions, PathTarget, Traversal};
use worktree_traversal::{TraversalProgress, TraversalTarget};
impl TryFrom<(&CharBag, &PathMatcher, proto::Entry)> for Entry {
    type Error = anyhow::Error;

    fn try_from(
        (root_char_bag, always_included, entry): (&CharBag, &PathMatcher, proto::Entry),
    ) -> Result<Self> {
        let kind = if entry.is_dir {
            EntryKind::Dir
        } else {
            EntryKind::File
        };

        let path =
            RelPath::from_proto(&entry.path).context("invalid relative path in proto message")?;
        let char_bag = char_bag_for_path(*root_char_bag, &path);
        let is_always_included = always_included.is_match(&path);
        Ok(Entry {
            id: ProjectEntryId::from_proto(entry.id),
            kind,
            path,
            inode: entry.inode,
            mtime: entry.mtime.map(|time| time.into()),
            size: entry.size.unwrap_or(0),
            canonical_path: entry
                .canonical_path
                .map(|path_string| Arc::from(PathBuf::from(path_string))),
            is_ignored: entry.is_ignored,
            is_hidden: entry.is_hidden,
            is_always_included,
            is_external: entry.is_external,
            is_private: false,
            char_bag,
            is_fifo: entry.is_fifo,
        })
    }
}

#[cfg(test)]
#[path = "worktree/tests.rs"]
mod tests;
