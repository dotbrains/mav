use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: ProjectEntryId,
    pub kind: EntryKind,
    pub path: Arc<RelPath>,
    pub inode: u64,
    pub mtime: Option<MTime>,

    pub canonical_path: Option<Arc<Path>>,
    /// Whether this entry is ignored by Git.
    ///
    /// We only scan ignored entries once the directory is expanded and
    /// exclude them from searches.
    pub is_ignored: bool,

    /// Whether this entry is hidden or inside hidden directory.
    ///
    /// We only scan hidden entries once the directory is expanded.
    pub is_hidden: bool,

    /// Whether this entry is always included in searches.
    ///
    /// This is used for entries that are always included in searches, even
    /// if they are ignored by git. Overridden by file_scan_exclusions.
    pub is_always_included: bool,

    /// Whether this entry's canonical path is outside of the worktree.
    /// This means the entry is only accessible from the worktree root via a
    /// symlink.
    ///
    /// We only scan entries outside of the worktree once the symlinked
    /// directory is expanded.
    pub is_external: bool,

    /// Whether this entry is considered to be a `.env` file.
    pub is_private: bool,
    /// The entry's size on disk, in bytes.
    pub size: u64,
    pub char_bag: CharBag,
    pub is_fifo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    UnloadedDir,
    PendingDir,
    Dir,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathChange {
    /// A filesystem entry was was created.
    Added,
    /// A filesystem entry was removed.
    Removed,
    /// A filesystem entry was updated.
    Updated,
    /// A filesystem entry was either updated or added. We don't know
    /// whether or not it already existed, because the path had not
    /// been loaded before the event.
    AddedOrUpdated,
    /// A filesystem entry was found during the initial scan of the worktree.
    Loaded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdatedGitRepository {
    /// ID of the repository's working directory.
    ///
    /// For a repo that's above the worktree root, this is the ID of the worktree root, and hence not unique.
    /// It's included here to aid the GitStore in detecting when a repository's working directory is renamed.
    pub work_directory_id: ProjectEntryId,
    pub old_work_directory_abs_path: Option<Arc<Path>>,
    pub new_work_directory_abs_path: Option<Arc<Path>>,
    /// For a normal git repository checkout, the absolute path to the .git directory.
    /// For a worktree, the absolute path to the worktree's subdirectory inside the .git directory.
    pub dot_git_abs_path: Option<Arc<Path>>,
    pub repository_dir_abs_path: Option<Arc<Path>>,
    pub common_dir_abs_path: Option<Arc<Path>>,
}

pub type UpdatedEntriesSet = Arc<[(Arc<RelPath>, ProjectEntryId, PathChange)]>;
pub type UpdatedGitRepositoriesSet = Arc<[UpdatedGitRepository]>;

#[derive(Clone, Debug)]
pub struct PathProgress<'a> {
    pub max_path: &'a RelPath,
}

#[derive(Clone, Debug)]
pub struct PathSummary<S> {
    pub max_path: Arc<RelPath>,
    pub item_summary: S,
}

impl<S: Summary> Summary for PathSummary<S> {
    type Context<'a> = S::Context<'a>;

    fn zero(cx: Self::Context<'_>) -> Self {
        Self {
            max_path: RelPath::empty_arc(),
            item_summary: S::zero(cx),
        }
    }

    fn add_summary(&mut self, rhs: &Self, cx: Self::Context<'_>) {
        self.max_path = rhs.max_path.clone();
        self.item_summary.add_summary(&rhs.item_summary, cx);
    }
}

impl<'a, S: Summary> sum_tree::Dimension<'a, PathSummary<S>> for PathProgress<'a> {
    fn zero(_: <PathSummary<S> as Summary>::Context<'_>) -> Self {
        Self {
            max_path: RelPath::empty(),
        }
    }

    fn add_summary(
        &mut self,
        summary: &'a PathSummary<S>,
        _: <PathSummary<S> as Summary>::Context<'_>,
    ) {
        self.max_path = summary.max_path.as_ref()
    }
}

impl<'a> sum_tree::Dimension<'a, PathSummary<GitSummary>> for GitSummary {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<GitSummary>, _: ()) {
        *self += summary.item_summary
    }
}

impl<'a>
    sum_tree::SeekTarget<'a, PathSummary<GitSummary>, Dimensions<TraversalProgress<'a>, GitSummary>>
    for PathTarget<'_>
{
    fn cmp(
        &self,
        cursor_location: &Dimensions<TraversalProgress<'a>, GitSummary>,
        _: (),
    ) -> Ordering {
        self.cmp_path(cursor_location.0.max_path)
    }
}

impl<'a, S: Summary> sum_tree::Dimension<'a, PathSummary<S>> for PathKey {
    fn zero(_: S::Context<'_>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<S>, _: S::Context<'_>) {
        self.0 = summary.max_path.clone();
    }
}

impl<'a, S: Summary> sum_tree::Dimension<'a, PathSummary<S>> for TraversalProgress<'a> {
    fn zero(_cx: S::Context<'_>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<S>, _: S::Context<'_>) {
        self.max_path = summary.max_path.as_ref();
    }
}

impl Entry {
    pub(super) fn new(
        path: Arc<RelPath>,
        metadata: &fs::Metadata,
        id: ProjectEntryId,
        root_char_bag: CharBag,
        canonical_path: Option<Arc<Path>>,
    ) -> Self {
        let char_bag = char_bag_for_path(root_char_bag, &path);
        Self {
            id,
            kind: if metadata.is_dir {
                EntryKind::PendingDir
            } else {
                EntryKind::File
            },
            path,
            inode: metadata.inode,
            mtime: Some(metadata.mtime),
            size: metadata.len,
            canonical_path,
            is_ignored: false,
            is_hidden: false,
            is_always_included: false,
            is_external: false,
            is_private: false,
            char_bag,
            is_fifo: metadata.is_fifo,
        }
    }

    pub fn is_created(&self) -> bool {
        self.mtime.is_some()
    }

    pub fn is_dir(&self) -> bool {
        self.kind.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.kind.is_file()
    }
}

impl EntryKind {
    pub fn is_dir(&self) -> bool {
        matches!(
            self,
            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir
        )
    }

    pub fn is_unloaded(&self) -> bool {
        matches!(self, EntryKind::UnloadedDir)
    }

    pub fn is_file(&self) -> bool {
        matches!(self, EntryKind::File)
    }
}

impl sum_tree::Item for Entry {
    type Summary = EntrySummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        let non_ignored_count = if self.is_ignored && !self.is_always_included {
            0
        } else {
            1
        };
        let file_count;
        let non_ignored_file_count;
        if self.is_file() {
            file_count = 1;
            non_ignored_file_count = non_ignored_count;
        } else {
            file_count = 0;
            non_ignored_file_count = 0;
        }

        EntrySummary {
            max_path: self.path.clone(),
            count: 1,
            non_ignored_count,
            file_count,
            non_ignored_file_count,
        }
    }
}

impl sum_tree::KeyedItem for Entry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.path.clone())
    }
}

#[derive(Clone, Debug)]
pub struct EntrySummary {
    pub(super) max_path: Arc<RelPath>,
    pub(super) count: usize,
    pub(super) non_ignored_count: usize,
    pub(super) file_count: usize,
    pub(super) non_ignored_file_count: usize,
}

impl Default for EntrySummary {
    fn default() -> Self {
        Self {
            max_path: Arc::from(RelPath::empty()),
            count: 0,
            non_ignored_count: 0,
            file_count: 0,
            non_ignored_file_count: 0,
        }
    }
}

impl sum_tree::ContextLessSummary for EntrySummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, rhs: &Self) {
        self.max_path = rhs.max_path.clone();
        self.count += rhs.count;
        self.non_ignored_count += rhs.non_ignored_count;
        self.file_count += rhs.file_count;
        self.non_ignored_file_count += rhs.non_ignored_file_count;
    }
}

#[derive(Clone, Debug)]
pub(super) struct PathEntry {
    pub(super) id: ProjectEntryId,
    pub(super) path: Arc<RelPath>,
    pub(super) is_ignored: bool,
    pub(super) scan_id: usize,
}

impl sum_tree::Item for PathEntry {
    type Summary = PathEntrySummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        PathEntrySummary { max_id: self.id }
    }
}

impl sum_tree::KeyedItem for PathEntry {
    type Key = ProjectEntryId;

    fn key(&self) -> Self::Key {
        self.id
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PathEntrySummary {
    pub(super) max_id: ProjectEntryId,
}

impl sum_tree::ContextLessSummary for PathEntrySummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.max_id = summary.max_id;
    }
}

impl<'a> sum_tree::Dimension<'a, PathEntrySummary> for ProjectEntryId {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a PathEntrySummary, _: ()) {
        *self = summary.max_id;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PathKey(pub Arc<RelPath>);

impl Default for PathKey {
    fn default() -> Self {
        Self(RelPath::empty_arc())
    }
}

impl<'a> sum_tree::Dimension<'a, EntrySummary> for PathKey {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a EntrySummary, _: ()) {
        self.0 = summary.max_path.clone();
    }
}
