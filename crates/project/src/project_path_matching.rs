use super::*;

/// Identifies a project group by a set of paths the workspaces in this group
/// have.
///
/// Paths are mapped to their main worktree path first so we can group
/// workspaces by main repos.
#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
pub struct ProjectGroupKey {
    /// The paths of the main worktrees for this project group.
    paths: PathList,
    host: Option<RemoteConnectionOptions>,
}

impl ProjectGroupKey {
    /// Creates a new `ProjectGroupKey` with the given path list.
    ///
    /// The path list should point to the git main worktree paths for a project.
    pub fn new(host: Option<RemoteConnectionOptions>, paths: PathList) -> Self {
        Self { paths, host }
    }

    pub fn from_project(project: &Project, cx: &App) -> Self {
        let paths = project.worktree_paths(cx);
        let host = project.remote_connection_options(cx);
        Self {
            paths: paths.main_worktree_path_list().clone(),
            host,
        }
    }

    pub fn from_worktree_paths(
        paths: &WorktreePaths,
        host: Option<RemoteConnectionOptions>,
    ) -> Self {
        Self {
            paths: paths.main_worktree_path_list().clone(),
            host,
        }
    }

    pub fn path_list(&self) -> &PathList {
        &self.paths
    }

    pub fn display_name(
        &self,
        path_detail_map: &std::collections::HashMap<PathBuf, usize>,
    ) -> SharedString {
        let mut names = Vec::with_capacity(self.paths.paths().len());
        for abs_path in self.paths.ordered_paths() {
            let detail = path_detail_map.get(abs_path).copied().unwrap_or(0);
            // Strip a `.git` extension for display (bare clones like `foo.git`
            // should display as `foo`, matching the titlebar).
            let display_path = if abs_path.extension() == Some(std::ffi::OsStr::new("git")) {
                std::borrow::Cow::Owned(abs_path.with_extension(""))
            } else {
                std::borrow::Cow::Borrowed(abs_path.as_path())
            };
            let suffix = path_suffix(&display_path, detail);
            if !suffix.is_empty() {
                names.push(suffix);
            }
        }
        if names.is_empty() {
            "Empty Workspace".into()
        } else {
            names.join(", ").into()
        }
    }

    pub fn host(&self) -> Option<RemoteConnectionOptions> {
        self.host.clone()
    }

    pub fn matches(&self, other: &ProjectGroupKey) -> bool {
        self.paths == other.paths
            && same_remote_connection_identity(self.host.as_ref(), other.host.as_ref())
    }
}

pub fn path_suffix(path: &Path, detail: usize) -> String {
    let mut components: Vec<_> = path
        .components()
        .rev()
        .filter_map(|component| match component {
            std::path::Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .take(detail + 1)
        .collect();
    components.reverse();
    components.join("/")
}

pub struct PathMatchCandidateSet {
    pub snapshot: Snapshot,
    pub include_ignored: bool,
    pub include_root_name: bool,
    pub candidates: Candidates,
}

pub enum Candidates {
    /// Only consider directories.
    Directories,
    /// Only consider files.
    Files,
    /// Consider directories and files.
    Entries,
}

impl<'a> fuzzy::PathMatchCandidateSet<'a> for PathMatchCandidateSet {
    type Candidates = PathMatchCandidateSetIter<'a>;

    fn id(&self) -> usize {
        self.snapshot.id().to_usize()
    }

    fn len(&self) -> usize {
        match self.candidates {
            Candidates::Files => {
                if self.include_ignored {
                    self.snapshot.file_count()
                } else {
                    self.snapshot.visible_file_count()
                }
            }

            Candidates::Directories => {
                if self.include_ignored {
                    self.snapshot.dir_count()
                } else {
                    self.snapshot.visible_dir_count()
                }
            }

            Candidates::Entries => {
                if self.include_ignored {
                    self.snapshot.entry_count()
                } else {
                    self.snapshot.visible_entry_count()
                }
            }
        }
    }

    fn prefix(&self) -> Arc<RelPath> {
        if self.snapshot.root_entry().is_some_and(|e| e.is_file()) || self.include_root_name {
            self.snapshot.root_name().into()
        } else {
            RelPath::empty_arc()
        }
    }

    fn root_is_file(&self) -> bool {
        self.snapshot.root_entry().is_some_and(|f| f.is_file())
    }

    fn path_style(&self) -> PathStyle {
        self.snapshot.path_style()
    }

    fn candidates(&'a self, start: usize) -> Self::Candidates {
        PathMatchCandidateSetIter {
            traversal: match self.candidates {
                Candidates::Directories => self.snapshot.directories(self.include_ignored, start),
                Candidates::Files => self.snapshot.files(self.include_ignored, start),
                Candidates::Entries => self.snapshot.entries(self.include_ignored, start),
            },
        }
    }
}

pub struct PathMatchCandidateSetIter<'a> {
    traversal: Traversal<'a>,
}

impl<'a> Iterator for PathMatchCandidateSetIter<'a> {
    type Item = fuzzy::PathMatchCandidate<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.traversal
            .next()
            .map(|entry| fuzzy::PathMatchCandidate {
                is_dir: entry.kind.is_dir(),
                path: &entry.path,
                char_bag: entry.char_bag,
            })
    }
}

impl<'a> fuzzy_nucleo::PathMatchCandidateSet<'a> for PathMatchCandidateSet {
    type Candidates = PathMatchCandidateSetNucleoIter<'a>;
    fn id(&self) -> usize {
        self.snapshot.id().to_usize()
    }
    fn len(&self) -> usize {
        match self.candidates {
            Candidates::Files => {
                if self.include_ignored {
                    self.snapshot.file_count()
                } else {
                    self.snapshot.visible_file_count()
                }
            }
            Candidates::Directories => {
                if self.include_ignored {
                    self.snapshot.dir_count()
                } else {
                    self.snapshot.visible_dir_count()
                }
            }
            Candidates::Entries => {
                if self.include_ignored {
                    self.snapshot.entry_count()
                } else {
                    self.snapshot.visible_entry_count()
                }
            }
        }
    }
    fn prefix(&self) -> Arc<RelPath> {
        if self.snapshot.root_entry().is_some_and(|e| e.is_file()) || self.include_root_name {
            self.snapshot.root_name().into()
        } else {
            RelPath::empty_arc()
        }
    }
    fn root_is_file(&self) -> bool {
        self.snapshot.root_entry().is_some_and(|f| f.is_file())
    }
    fn path_style(&self) -> PathStyle {
        self.snapshot.path_style()
    }
    fn candidates(&'a self, start: usize) -> Self::Candidates {
        PathMatchCandidateSetNucleoIter {
            traversal: match self.candidates {
                Candidates::Directories => self.snapshot.directories(self.include_ignored, start),
                Candidates::Files => self.snapshot.files(self.include_ignored, start),
                Candidates::Entries => self.snapshot.entries(self.include_ignored, start),
            },
        }
    }
}

pub struct PathMatchCandidateSetNucleoIter<'a> {
    traversal: Traversal<'a>,
}

impl<'a> Iterator for PathMatchCandidateSetNucleoIter<'a> {
    type Item = fuzzy_nucleo::PathMatchCandidate<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.traversal
            .next()
            .map(|entry| fuzzy_nucleo::PathMatchCandidate {
                is_dir: entry.kind.is_dir(),
                path: &entry.path,
                char_bag: entry.char_bag,
            })
    }
}
