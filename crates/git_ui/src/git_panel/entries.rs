use super::*;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct SerializedGitPanel {
    #[serde(default)]
    pub(super) signoff_enabled: bool,
    #[serde(default)]
    pub(super) commit_messages: BTreeMap<String, SerializedCommitMessage>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct SerializedCommitMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) original_message: Option<String>,
    #[serde(default)]
    pub(super) amend_pending: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum GitPanelTab {
    Changes,
    History,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub(super) enum Section {
    Conflict,
    Tracked,
    New,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct GitHeaderEntry {
    pub(super) header: Section,
}

impl GitHeaderEntry {
    pub(super) fn contains(&self, status_entry: &GitStatusEntry, repo: &Repository) -> bool {
        let this = &self.header;
        let status = status_entry.status;
        match this {
            Section::Conflict => {
                repo.had_conflict_on_last_merge_head_change(&status_entry.repo_path)
            }
            Section::Tracked => !status.is_created(),
            Section::New => status.is_created(),
        }
    }
    pub(super) fn title(&self) -> &'static str {
        match self.header {
            Section::Conflict => "Conflicts",
            Section::Tracked => "Tracked",
            Section::New => "Untracked",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) enum GitListEntry {
    Status(GitStatusEntry),
    TreeStatus(GitTreeStatusEntry),
    Directory(GitTreeDirEntry),
    Header(GitHeaderEntry),
}

impl GitListEntry {
    pub(super) fn status_entry(&self) -> Option<&GitStatusEntry> {
        match self {
            GitListEntry::Status(entry) => Some(entry),
            GitListEntry::TreeStatus(entry) => Some(&entry.entry),
            _ => None,
        }
    }

    pub(super) fn directory_entry(&self) -> Option<&GitTreeDirEntry> {
        match self {
            GitListEntry::Directory(entry) => Some(entry),
            _ => None,
        }
    }

    /// Returns the tree indentation depth for this entry.
    pub(super) fn depth(&self) -> usize {
        match self {
            GitListEntry::Directory(dir) => dir.depth,
            GitListEntry::TreeStatus(status) => status.depth,
            _ => 0,
        }
    }
}

pub(super) enum GitPanelViewMode {
    Flat,
    Tree(TreeViewState),
}

impl GitPanelViewMode {
    pub(super) fn from_settings(cx: &App) -> Self {
        if GitPanelSettings::get_global(cx).tree_view {
            GitPanelViewMode::Tree(TreeViewState::default())
        } else {
            GitPanelViewMode::Flat
        }
    }

    pub(super) fn tree_state(&self) -> Option<&TreeViewState> {
        match self {
            GitPanelViewMode::Tree(state) => Some(state),
            GitPanelViewMode::Flat => None,
        }
    }

    pub(super) fn tree_state_mut(&mut self) -> Option<&mut TreeViewState> {
        match self {
            GitPanelViewMode::Tree(state) => Some(state),
            GitPanelViewMode::Flat => None,
        }
    }
}

#[derive(Default)]
pub(super) struct TreeViewState {
    // Maps visible index to actual entry index.
    // Length equals the number of visible entries.
    // This is needed because some entries, like collapsed directories, may be hidden.
    pub(super) logical_indices: Vec<usize>,
    pub(super) expanded_dirs: HashMap<RepoPath, bool>,
    pub(super) directory_descendants: HashMap<TreeKey, Vec<GitStatusEntry>>,
}

impl TreeViewState {
    pub(super) fn build_tree_entries(
        &mut self,
        section: Section,
        mut entries: Vec<GitStatusEntry>,
        seen_directories: &mut HashSet<TreeKey>,
    ) -> Vec<(GitListEntry, bool)> {
        if entries.is_empty() {
            return Vec::new();
        }

        entries.sort_by(|a, b| a.repo_path.cmp(&b.repo_path));

        let mut root = TreeNode::default();
        for entry in entries {
            let components: Vec<&str> = entry.repo_path.components().collect();
            if components.is_empty() {
                root.files.push(entry);
                continue;
            }

            let mut current = &mut root;
            let mut current_path = String::new();

            for (ix, component) in components.iter().enumerate() {
                if ix == components.len() - 1 {
                    current.files.push(entry.clone());
                } else {
                    if !current_path.is_empty() {
                        current_path.push('/');
                    }
                    current_path.push_str(component);
                    let dir_path = RepoPath::new(&current_path)
                        .expect("repo path from status entry component");

                    let component = SharedString::from(component.to_string());

                    current = current
                        .children
                        .entry(component.clone())
                        .or_insert_with(|| TreeNode {
                            name: component,
                            path: Some(dir_path),
                            ..Default::default()
                        });
                }
            }
        }

        let (flattened, _) = self.flatten_tree(&root, section, 0, seen_directories);
        flattened
    }

    fn flatten_tree(
        &mut self,
        node: &TreeNode,
        section: Section,
        depth: usize,
        seen_directories: &mut HashSet<TreeKey>,
    ) -> (Vec<(GitListEntry, bool)>, Vec<GitStatusEntry>) {
        let mut all_statuses = Vec::new();
        let mut flattened = Vec::new();

        for child in node.children.values() {
            let (terminal, name) = Self::compact_directory_chain(child);
            let Some(path) = terminal.path.clone().or_else(|| child.path.clone()) else {
                continue;
            };
            let (child_flattened, mut child_statuses) =
                self.flatten_tree(terminal, section, depth + 1, seen_directories);
            let key = TreeKey { section, path };
            let expanded = *self.expanded_dirs.get(&key.path).unwrap_or(&true);
            self.expanded_dirs.entry(key.path.clone()).or_insert(true);
            seen_directories.insert(key.clone());

            self.directory_descendants
                .insert(key.clone(), child_statuses.clone());

            flattened.push((
                GitListEntry::Directory(GitTreeDirEntry {
                    key,
                    name,
                    depth,
                    expanded,
                }),
                true,
            ));

            if expanded {
                flattened.extend(child_flattened);
            } else {
                flattened.extend(child_flattened.into_iter().map(|(child, _)| (child, false)));
            }

            all_statuses.append(&mut child_statuses);
        }

        for file in &node.files {
            all_statuses.push(file.clone());
            flattened.push((
                GitListEntry::TreeStatus(GitTreeStatusEntry {
                    entry: file.clone(),
                    depth,
                }),
                true,
            ));
        }

        (flattened, all_statuses)
    }

    fn compact_directory_chain(mut node: &TreeNode) -> (&TreeNode, SharedString) {
        let mut parts = vec![node.name.clone()];
        while node.files.is_empty() && node.children.len() == 1 {
            let Some(child) = node.children.values().next() else {
                continue;
            };
            if child.path.is_none() {
                break;
            }
            parts.push(child.name.clone());
            node = child;
        }
        let name = parts.join("/");
        (node, SharedString::from(name))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct GitTreeStatusEntry {
    pub(super) entry: GitStatusEntry,
    pub(super) depth: usize,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub(super) struct TreeKey {
    pub(super) section: Section,
    pub(super) path: RepoPath,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct GitTreeDirEntry {
    pub(super) key: TreeKey,
    pub(super) name: SharedString,
    pub(super) depth: usize,
    // staged_state: ToggleState,
    pub(super) expanded: bool,
}

#[derive(Default)]
struct TreeNode {
    name: SharedString,
    path: Option<RepoPath>,
    children: BTreeMap<SharedString, TreeNode>,
    files: Vec<GitStatusEntry>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct GitStatusEntry {
    pub(crate) repo_path: RepoPath,
    pub(crate) status: FileStatus,
    pub(crate) staging: StageStatus,
    pub(crate) diff_stat: Option<DiffStat>,
}

impl GitStatusEntry {
    pub(super) fn display_name(&self, path_style: PathStyle) -> String {
        self.repo_path
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| self.repo_path.display(path_style).to_string())
    }

    pub(super) fn parent_dir(&self, path_style: PathStyle) -> Option<String> {
        self.repo_path
            .parent()
            .map(|parent| parent.display(path_style).to_string())
    }
}

pub(super) struct TruncatedPatch {
    pub(super) header: String,
    pub(super) hunks: Vec<String>,
    pub(super) hunks_to_keep: usize,
}

impl TruncatedPatch {
    pub(super) fn from_unified_diff(patch_str: &str) -> Option<Self> {
        let lines: Vec<&str> = patch_str.lines().collect();
        if lines.len() < 2 {
            return None;
        }
        let header = format!("{}\n{}\n", lines[0], lines[1]);
        let mut hunks = Vec::new();
        let mut current_hunk = String::new();
        for line in &lines[2..] {
            if line.starts_with("@@") {
                if !current_hunk.is_empty() {
                    hunks.push(current_hunk);
                }
                current_hunk = format!("{}\n", line);
            } else if !current_hunk.is_empty() {
                current_hunk.push_str(line);
                current_hunk.push('\n');
            }
        }
        if !current_hunk.is_empty() {
            hunks.push(current_hunk);
        }
        if hunks.is_empty() {
            return None;
        }
        let hunks_to_keep = hunks.len();
        Some(TruncatedPatch {
            header,
            hunks,
            hunks_to_keep,
        })
    }
    pub(super) fn calculate_size(&self) -> usize {
        let mut size = self.header.len();
        for (i, hunk) in self.hunks.iter().enumerate() {
            if i < self.hunks_to_keep {
                size += hunk.len();
            }
        }
        size
    }
    pub(super) fn to_string(&self) -> String {
        let mut out = self.header.clone();
        for (i, hunk) in self.hunks.iter().enumerate() {
            if i < self.hunks_to_keep {
                out.push_str(hunk);
            }
        }
        let skipped_hunks = self.hunks.len() - self.hunks_to_keep;
        if skipped_hunks > 0 {
            out.push_str(&format!("[...skipped {} hunks...]\n", skipped_hunks));
        }
        out
    }
}
