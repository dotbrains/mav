use super::*;

pub(super) struct BackgroundScanner {
    pub(super) state: async_lock::Mutex<BackgroundScannerState>,
    pub(super) fs: Arc<dyn Fs>,
    pub(super) fs_case_sensitive: bool,
    pub(super) status_updates_tx: UnboundedSender<ScanState>,
    pub(super) executor: BackgroundExecutor,
    pub(super) scan_requests_rx: async_channel::Receiver<ScanRequest>,
    pub(super) path_prefixes_to_scan_rx: async_channel::Receiver<PathPrefixScanRequest>,
    pub(super) next_entry_id: Arc<AtomicUsize>,
    pub(super) phase: BackgroundScannerPhase,
    pub(super) watcher: Arc<dyn Watcher>,
    pub(super) settings: WorktreeSettings,
    pub(super) share_private_files: bool,
    pub(super) track_git_repositories: bool,
    /// Whether this is a single-file worktree (root is a file, not a directory).
    /// Used to determine if we should give up after repeated canonicalization failures.
    pub(super) is_single_file: bool,
    pub(super) defer_watch: bool,
}

#[derive(Copy, Clone, PartialEq)]
pub(super) enum BackgroundScannerPhase {
    InitialScan,
    EventsReceivedDuringInitialScan,
    Events,
}

mod events;
mod git_repositories;
mod global_gitignore;
mod ignore_updates;
mod reload;
mod requests;
mod run_loop;
mod scan;
mod utilities;
