use super::*;

#[derive(Debug)]
pub enum CreatedEntry {
    /// Got created and indexed by the worktree, receiving a corresponding entry.
    Included(Entry),
    /// Got created, but not indexed due to falling under exclusion filters.
    Excluded { abs_path: PathBuf },
}

#[derive(Debug)]
pub struct LoadedFile {
    pub file: Arc<File>,
    pub text: String,
    pub encoding: &'static Encoding,
    pub has_bom: bool,
}

pub struct LoadedBinaryFile {
    pub file: Arc<File>,
    pub content: Vec<u8>,
}

impl fmt::Debug for LoadedBinaryFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedBinaryFile")
            .field("file", &self.file)
            .field("content_bytes", &self.content.len())
            .finish()
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum WorkDirectory {
    InProject {
        relative_path: Arc<RelPath>,
    },
    AboveProject {
        absolute_path: Arc<Path>,
        location_in_repo: Arc<Path>,
    },
}

impl WorkDirectory {
    pub(crate) fn path_key(&self) -> PathKey {
        match self {
            WorkDirectory::InProject { relative_path } => PathKey(relative_path.clone()),
            WorkDirectory::AboveProject { .. } => PathKey(RelPath::empty_arc()),
        }
    }

    /// Returns true if the given path is a child of the work directory.
    ///
    /// Note that the path may not be a member of this repository, if there
    /// is a repository in a directory between these two paths
    /// external .git folder in a parent folder of the project root.
    #[track_caller]
    pub fn directory_contains(&self, path: &RelPath) -> bool {
        match self {
            WorkDirectory::InProject { relative_path } => path.starts_with(relative_path),
            WorkDirectory::AboveProject { .. } => true,
        }
    }
}

impl Default for WorkDirectory {
    fn default() -> Self {
        Self::InProject {
            relative_path: Arc::from(RelPath::empty()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    UpdatedEntries(UpdatedEntriesSet),
    UpdatedGitRepositories(UpdatedGitRepositoriesSet),
    UpdatedRootRepoCommonDir {
        old: Option<Arc<SanitizedPath>>,
    },
    DeletedEntry(ProjectEntryId),
    /// The worktree root itself has been deleted (for single-file worktrees)
    Deleted,
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectEntryId(pub(crate) usize);

impl ProjectEntryId {
    pub const MAX: Self = Self(usize::MAX);
    pub const MIN: Self = Self(usize::MIN);

    pub fn new(counter: &AtomicUsize) -> Self {
        Self(counter.fetch_add(1, SeqCst))
    }

    pub fn from_proto(id: u64) -> Self {
        Self(id as usize)
    }

    pub fn to_proto(self) -> u64 {
        self.0 as u64
    }

    pub fn from_usize(id: usize) -> Self {
        ProjectEntryId(id)
    }

    pub fn to_usize(self) -> usize {
        self.0
    }
}

#[cfg(feature = "test-support")]
impl CreatedEntry {
    pub fn into_included(self) -> Option<Entry> {
        match self {
            CreatedEntry::Included(entry) => Some(entry),
            CreatedEntry::Excluded { .. } => None,
        }
    }
}
