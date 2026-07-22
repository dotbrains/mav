use super::*;

pub struct RecentProjectsDelegate {
    workspace: WeakEntity<Workspace>,
    open_folders: Vec<OpenFolderEntry>,
    window_project_groups: Vec<ProjectGroupKey>,
    workspaces: Vec<RecentWorkspace>,
    filtered_entries: Vec<ProjectPickerEntry>,
    selected_index: usize,
    render_paths: bool,
    create_new_window: bool,
    snap_selection_to_first_non_header_match: bool,
    focus_handle: FocusHandle,
    style: ProjectPickerStyle,
    actions_menu_handle: PopoverMenuHandle<ContextMenu>,
}

impl RecentProjectsDelegate {
    fn new(
        workspace: WeakEntity<Workspace>,
        create_new_window: bool,
        focus_handle: FocusHandle,
        open_folders: Vec<OpenFolderEntry>,
        window_project_groups: Vec<ProjectGroupKey>,
        style: ProjectPickerStyle,
    ) -> Self {
        let render_paths = style == ProjectPickerStyle::Modal;
        Self {
            workspace,
            open_folders,
            window_project_groups,
            workspaces: Vec::new(),
            filtered_entries: Vec::new(),
            selected_index: 0,
            create_new_window,
            render_paths,
            snap_selection_to_first_non_header_match: true,
            focus_handle,
            style,
            actions_menu_handle: PopoverMenuHandle::default(),
        }
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<RecentWorkspace>) {
        self.workspaces = workspaces;
    }

    fn filtered_entries_include_remote_project(&self) -> bool {
        self.filtered_entries
            .iter()
            .any(|entry| self.entry_is_remote_project(entry))
    }

    fn entry_is_remote_project(&self, entry: &ProjectPickerEntry) -> bool {
        match entry {
            ProjectPickerEntry::Header(_) => false,
            ProjectPickerEntry::OpenFolder { index, .. } => self
                .open_folders
                .get(*index)
                .is_some_and(|folder| folder.connection_options.is_some()),
            ProjectPickerEntry::ProjectGroup(hit) => self
                .window_project_groups
                .get(hit.candidate_id)
                .is_some_and(|key| key.host().is_some()),
            ProjectPickerEntry::RecentProject(hit) => self
                .workspaces
                .get(hit.candidate_id)
                .is_some_and(|workspace| {
                    matches!(workspace.location, SerializedWorkspaceLocation::Remote(_))
                }),
        }
    }
}
impl EventEmitter<DismissEvent> for RecentProjectsDelegate {}
