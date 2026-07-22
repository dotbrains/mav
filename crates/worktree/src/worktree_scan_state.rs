use super::*;

pub(super) struct PathPrefixScanRequest {
    pub(super) path: Arc<RelPath>,
    pub(super) done: SmallVec<[barrier::Sender; 1]>,
}

pub(super) struct ScanRequest {
    pub(super) relative_paths: Vec<Arc<RelPath>>,
    pub(super) done: SmallVec<[barrier::Sender; 1]>,
}

pub(super) struct BackgroundScannerState {
    pub(super) snapshot: LocalSnapshot,
    pub(super) symlink_paths_by_target: HashMap<Arc<Path>, SmallVec<[Arc<RelPath>; 1]>>,
    pub(super) scanned_dirs: HashSet<ProjectEntryId>,
    pub(super) watched_dir_abs_paths_by_entry_id: HashMap<ProjectEntryId, Arc<Path>>,
    pub(super) path_prefixes_to_scan: HashSet<Arc<RelPath>>,
    pub(super) paths_to_scan: HashSet<Arc<RelPath>>,
    /// The ids of all of the entries that were removed from the snapshot
    /// as part of the current update. These entry ids may be re-used
    /// if the same inode is discovered at a new path, or if the given
    /// path is re-created after being deleted.
    pub(super) removed_entries: HashMap<u64, Entry>,
    pub(super) changed_paths: Vec<Arc<RelPath>>,
    pub(super) prev_snapshot: Snapshot,
    pub(super) scanning_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct EventRoot {
    pub(super) path: Arc<RelPath>,
    pub(super) was_rescanned: bool,
}

pub(super) enum ScanState {
    Started,
    Updated {
        snapshot: LocalSnapshot,
        changes: UpdatedEntriesSet,
        barrier: SmallVec<[barrier::Sender; 1]>,
        scanning: bool,
    },
    RootUpdated {
        new_path: Arc<SanitizedPath>,
    },
    RootDeleted,
}

pub(super) struct UpdateObservationState {
    pub(super) snapshots_tx: mpsc::UnboundedSender<(LocalSnapshot, UpdatedEntriesSet)>,
    pub(super) resume_updates: watch::Sender<()>,
    pub(super) _maintain_remote_snapshot: Task<Option<()>>,
}
