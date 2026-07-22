use super::*;

pub struct LocalWorktree {
    pub(super) snapshot: LocalSnapshot,
    pub(super) scan_requests_tx: async_channel::Sender<ScanRequest>,
    pub(super) path_prefixes_to_scan_tx: async_channel::Sender<PathPrefixScanRequest>,
    pub(super) is_scanning: (watch::Sender<bool>, watch::Receiver<bool>),
    pub(super) snapshot_subscriptions: VecDeque<(usize, oneshot::Sender<()>)>,
    pub(super) _background_scanner_tasks: Vec<Task<()>>,
    pub(super) update_observer: Option<UpdateObservationState>,
    pub(super) fs: Arc<dyn Fs>,
    pub(super) fs_case_sensitive: bool,
    pub(super) visible: bool,
    pub(super) next_entry_id: Arc<AtomicUsize>,
    pub(super) settings: WorktreeSettings,
    pub(super) share_private_files: bool,
    pub(super) scanning_enabled: bool,
    pub(super) force_defer_watch: bool,
}

pub struct RemoteWorktree {
    pub(super) snapshot: Snapshot,
    pub(super) background_snapshot: Arc<Mutex<(Snapshot, Vec<proto::UpdateWorktree>)>>,
    pub(super) project_id: u64,
    pub(super) client: AnyProtoClient,
    pub(super) file_scan_inclusions: PathMatcher,
    pub(super) updates_tx: Option<UnboundedSender<proto::UpdateWorktree>>,
    pub(super) update_observer: Option<mpsc::UnboundedSender<proto::UpdateWorktree>>,
    pub(super) snapshot_subscriptions: VecDeque<(usize, oneshot::Sender<()>)>,
    pub(super) replica_id: ReplicaId,
    pub(super) visible: bool,
    pub(super) disconnected: bool,
    pub(super) received_initial_update: bool,
}

#[derive(Clone)]
pub struct Snapshot {
    pub(super) id: WorktreeId,
    /// The absolute path of the worktree root.
    pub(super) abs_path: Arc<SanitizedPath>,
    pub(super) path_style: PathStyle,
    pub(super) root_name: Arc<RelPath>,
    pub(super) root_char_bag: CharBag,
    pub(super) entries_by_path: SumTree<Entry>,
    pub(super) entries_by_id: SumTree<PathEntry>,
    pub(super) root_repo_common_dir: Option<Arc<SanitizedPath>>,
    pub(super) always_included_entries: Vec<Arc<RelPath>>,

    /// A number that increases every time the worktree begins scanning
    /// a set of paths from the filesystem. This scanning could be caused
    /// by some operation performed on the worktree, such as reading or
    /// writing a file, or by an event reported by the filesystem.
    pub(super) scan_id: usize,

    /// The latest scan id that has completed, and whose preceding scans
    /// have all completed. The current `scan_id` could be more than one
    /// greater than the `completed_scan_id` if operations are performed
    /// on the worktree while it is processing a file-system event.
    pub(super) completed_scan_id: usize,
}

#[derive(Clone)]
pub struct LocalSnapshot {
    pub(super) snapshot: Snapshot,
    pub(super) global_gitignore: Option<Arc<Gitignore>>,
    /// Exclude files for all git repositories in the worktree, indexed by their absolute path.
    /// The boolean indicates whether the gitignore needs to be updated.
    pub(super) repo_exclude_by_work_dir_abs_path: HashMap<Arc<Path>, (Arc<Gitignore>, bool)>,
    /// All of the gitignore files in the worktree, indexed by their absolute path.
    /// The boolean indicates whether the gitignore needs to be updated.
    pub(super) ignores_by_parent_abs_path: HashMap<Arc<Path>, (Arc<Gitignore>, bool)>,
    /// All of the git repositories in the worktree, indexed by the project entry
    /// id of their parent directory.
    pub(super) git_repositories: TreeMap<ProjectEntryId, LocalRepositoryEntry>,
    /// The file handle of the worktree root
    /// (so we can find it after it's been moved)
    pub(super) root_file_handle: Option<Arc<dyn fs::FileHandle>>,
    /// Maps canonical absolute paths of externally watched symlinked directories
    /// to their relative paths within the worktree, used to translate FSEvents
    /// canonical-path events back to worktree-relative paths.
    pub(super) external_canonical_to_relative: BTreeMap<Arc<Path>, Arc<RelPath>>,
}
