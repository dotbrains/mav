use super::*;

pub(super) struct ScanJob {
    pub(super) abs_path: Arc<Path>,
    pub(super) path: Arc<RelPath>,
    pub(super) ignore_stack: IgnoreStack,
    pub(super) scan_queue: Sender<ScanJob>,
    pub(super) ancestor_inodes: TreeSet<u64>,
    pub(super) is_external: bool,
}

pub(super) struct UpdateIgnoreStatusJob {
    pub(super) abs_path: Arc<Path>,
    pub(super) ignore_stack: IgnoreStack,
    pub(super) ignore_queue: Sender<UpdateIgnoreStatusJob>,
    pub(super) scan_queue: Sender<ScanJob>,
}
