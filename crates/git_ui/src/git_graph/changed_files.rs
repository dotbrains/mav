use super::*;

#[derive(Clone)]
pub(super) struct ChangedFileEntry {
    pub(super) status: FileStatus,
    pub(super) file_name: SharedString,
    pub(super) dir_path: SharedString,
    pub(super) repo_path: RepoPath,
}

impl ChangedFileEntry {
    pub(super) fn from_commit_file(file: &CommitFile, _cx: &App) -> Self {
        let file_name: SharedString = file
            .path
            .file_name()
            .map(|n| n.to_string())
            .unwrap_or_default()
            .into();
        let dir_path: SharedString = file
            .path
            .parent()
            .map(|p| p.as_unix_str().to_string())
            .unwrap_or_default()
            .into();

        let status_code = match (&file.old_text, &file.new_text) {
            (None, Some(_)) => StatusCode::Added,
            (Some(_), None) => StatusCode::Deleted,
            _ => StatusCode::Modified,
        };

        let status = FileStatus::Tracked(TrackedStatus {
            index_status: status_code,
            worktree_status: StatusCode::Unmodified,
        });

        Self {
            status,
            file_name,
            dir_path,
            repo_path: file.path.clone(),
        }
    }

    fn open_in_commit_view(
        &self,
        commit_sha: &SharedString,
        repository: &WeakEntity<Repository>,
        workspace: &WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        CommitView::open(
            commit_sha.to_string(),
            repository.clone(),
            workspace.clone(),
            None,
            Some(self.repo_path.clone()),
            window,
            cx,
        );
    }

    pub(super) fn render(
        &self,
        ix: usize,
        depth: usize,
        directory_label: Option<SharedString>,
        commit_sha: SharedString,
        repository: WeakEntity<Repository>,
        workspace: WeakEntity<Workspace>,
        _cx: &App,
    ) -> AnyElement {
        const TREE_INDENT: f32 = 12.0;

        let file_name = self.file_name.clone();
        let dir_path = self.dir_path.clone();

        ListItem::new(("changed-file", ix))
            .spacing(ListItemSpacing::Sparse)
            .indent_level(depth)
            .indent_step_size(px(TREE_INDENT))
            .start_slot(git_status_icon(self.status))
            .child(
                Label::new(file_name.clone())
                    .size(LabelSize::Small)
                    .truncate(),
            )
            .when_some(directory_label, |this, directory_label| {
                this.child(
                    Label::new(directory_label)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate_start(),
                )
            })
            .tooltip({
                let meta = if dir_path.is_empty() {
                    file_name
                } else {
                    format!("{}/{}", dir_path, file_name).into()
                };
                move |_, cx| Tooltip::with_meta("View Changes", None, meta.clone(), cx)
            })
            .on_click({
                let entry = self.clone();
                move |_, window, cx| {
                    entry.open_in_commit_view(&commit_sha, &repository, &workspace, window, cx);
                }
            })
            .into_any_element()
    }
}

pub(super) enum ChangedFileTreeEntry {
    Directory(ChangedFileDirectoryEntry),
    File(ChangedFileTreeStatusEntry),
}

pub(super) struct ChangedFileTreeStatusEntry {
    pub(super) entry: ChangedFileEntry,
    pub(super) depth: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ChangedFilesViewMode {
    Flat,
    #[default]
    Tree,
}

impl ChangedFilesViewMode {
    pub(super) fn toggled(self) -> Self {
        match self {
            Self::Flat => Self::Tree,
            Self::Tree => Self::Flat,
        }
    }

    pub(super) fn is_tree(self) -> bool {
        matches!(self, Self::Tree)
    }
}

pub(super) struct ChangedFileDirectoryEntry {
    path: RepoPath,
    name: SharedString,
    depth: usize,
    expanded: bool,
}

impl ChangedFileDirectoryEntry {
    pub(super) fn render(
        &self,
        ix: usize,
        git_graph: WeakEntity<GitGraph>,
        cx: &App,
    ) -> AnyElement {
        const TREE_INDENT: f32 = 12.0;

        let path = self.path.clone();
        let expanded = self.expanded;
        let folder_icon = FileIcons::get_folder_icon(expanded, path.as_std_path(), cx)
            .map(|icon| {
                Icon::from_path(icon)
                    .size(IconSize::Small)
                    .color(Color::Muted)
            })
            .unwrap_or_else(|| {
                let icon = if expanded {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                };
                Icon::new(icon).size(IconSize::Small).color(Color::Muted)
            });

        ListItem::new(("changed-file-dir", ix))
            .spacing(ListItemSpacing::Sparse)
            .indent_level(self.depth)
            .indent_step_size(px(TREE_INDENT))
            .toggle(Some(expanded))
            .always_show_disclosure_icon(true)
            .on_toggle({
                let path = path.clone();
                let git_graph = git_graph.clone();
                move |_, _, cx| {
                    git_graph
                        .update(cx, |git_graph, cx| {
                            git_graph
                                .changed_files_expanded_dirs
                                .insert(path.clone(), !expanded);
                            cx.notify();
                        })
                        .ok();
                }
            })
            .start_slot(folder_icon)
            .child(
                Label::new(self.name.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            )
            .tooltip({
                let name = self.name.clone();
                move |_, cx| Tooltip::with_meta("Toggle Folder", None, name.clone(), cx)
            })
            .on_click(move |_, _, cx| {
                git_graph
                    .update(cx, |git_graph, cx| {
                        git_graph
                            .changed_files_expanded_dirs
                            .insert(path.clone(), !expanded);
                        cx.notify();
                    })
                    .ok();
            })
            .into_any_element()
    }
}

#[derive(Default)]
struct ChangedFileTreeNode {
    name: SharedString,
    path: Option<RepoPath>,
    children: BTreeMap<SharedString, ChangedFileTreeNode>,
    files: Vec<ChangedFileEntry>,
}

pub(super) fn build_changed_file_tree_entries(
    mut files: Vec<ChangedFileEntry>,
    expanded_dirs: &HashMap<RepoPath, bool>,
) -> Vec<ChangedFileTreeEntry> {
    files.sort_by(|a, b| a.repo_path.cmp(&b.repo_path));

    let mut root = ChangedFileTreeNode::default();
    for file in files {
        let components: Vec<&str> = file.repo_path.components().collect();
        if components.is_empty() {
            root.files.push(file);
            continue;
        }

        let mut current = &mut root;
        let mut current_path = String::new();

        for (ix, component) in components.iter().enumerate() {
            if ix == components.len() - 1 {
                current.files.push(file.clone());
            } else {
                if !current_path.is_empty() {
                    current_path.push('/');
                }
                current_path.push_str(component);

                let Ok(dir_path) = RepoPath::new(&current_path) else {
                    continue;
                };
                let component = SharedString::from(component.to_string());

                current = current
                    .children
                    .entry(component.clone())
                    .or_insert_with(|| ChangedFileTreeNode {
                        name: component,
                        path: Some(dir_path),
                        ..Default::default()
                    });
            }
        }
    }

    flatten_changed_file_tree(&root, 0, expanded_dirs)
}

fn flatten_changed_file_tree(
    node: &ChangedFileTreeNode,
    depth: usize,
    expanded_dirs: &HashMap<RepoPath, bool>,
) -> Vec<ChangedFileTreeEntry> {
    let mut entries = Vec::new();

    for child in node.children.values() {
        let (terminal, name) = compact_changed_file_directory_chain(child);
        let Some(path) = terminal.path.clone().or_else(|| child.path.clone()) else {
            continue;
        };
        let expanded = *expanded_dirs.get(&path).unwrap_or(&true);
        let child_entries = flatten_changed_file_tree(terminal, depth + 1, expanded_dirs);

        entries.push(ChangedFileTreeEntry::Directory(ChangedFileDirectoryEntry {
            path,
            name,
            depth,
            expanded,
        }));

        if expanded {
            entries.extend(child_entries);
        }
    }

    entries.extend(
        node.files
            .iter()
            .cloned()
            .map(|entry| ChangedFileTreeEntry::File(ChangedFileTreeStatusEntry { entry, depth })),
    );
    entries
}

fn compact_changed_file_directory_chain(
    mut node: &ChangedFileTreeNode,
) -> (&ChangedFileTreeNode, SharedString) {
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
    (node, SharedString::from(parts.join("/")))
}
