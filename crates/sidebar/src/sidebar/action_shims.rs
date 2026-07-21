use super::*;

impl Sidebar {
    pub(super) fn rename_selected_thread(
        &mut self,
        _: &RenameSelectedThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else {
            return;
        };
        let Some(ListEntry::Thread(thread)) = self.contents.entries.get(ix) else {
            return;
        };
        let thread_id = thread.metadata.thread_id;
        let title = thread.metadata.display_title();
        self.start_renaming_thread(ix, thread_id, title, window, cx);
    }

    pub(super) fn render_recent_projects_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let multi_workspace = self.multi_workspace.upgrade();

        let workspace = multi_workspace
            .as_ref()
            .map(|mw| mw.read(cx).workspace().downgrade());

        let focus_handle = workspace
            .as_ref()
            .and_then(|ws| ws.upgrade())
            .map(|w| w.read(cx).focus_handle(cx))
            .unwrap_or_else(|| cx.focus_handle());

        let window_project_groups: Vec<ProjectGroupKey> = multi_workspace
            .as_ref()
            .map(|mw| mw.read(cx).project_group_keys())
            .unwrap_or_default();

        let popover_handle = self.recent_projects_popover_handle.clone();

        PopoverMenu::new("sidebar-recent-projects-menu")
            .with_handle(popover_handle)
            .menu(move |window, cx| {
                workspace.as_ref().map(|ws| {
                    SidebarRecentProjects::popover(
                        ws.clone(),
                        window_project_groups.clone(),
                        focus_handle.clone(),
                        window,
                        cx,
                    )
                })
            })
            .trigger_with_tooltip(
                IconButton::new("open-project", IconName::FolderAdd)
                    .icon_size(IconSize::Small)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent)),
                |_window, cx| Tooltip::for_action("Add Project", &OpenRecent::default(), cx),
            )
            .offset(gpui::Point {
                x: px(-2.0),
                y: px(-2.0),
            })
            .anchor(gpui::Anchor::BottomRight)
    }

    pub(super) fn new_thread_in_group(
        &mut self,
        _: &NewThreadInGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.selected_group_key() {
            self.set_group_expanded(&key, true, cx);
            self.selection = None;
            if let Some(workspace) = self.workspace_for_group(&key, cx) {
                self.create_new_entry(&workspace, window, cx);
            } else {
                self.open_workspace_and_create_entry(
                    &key,
                    NewEntryTarget::LastCreatedKind,
                    window,
                    cx,
                );
            }
        } else if let Some(workspace) = self.active_workspace(cx) {
            self.create_new_entry(&workspace, window, cx);
        }
    }

    pub(super) fn new_terminal_thread(
        &mut self,
        _: &NewTerminalThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();

        if let Some(key) = self.selected_group_key() {
            self.set_group_expanded(&key, true, cx);
            self.selection = None;
            if let Some(workspace) = self.workspace_for_group(&key, cx) {
                self.create_new_terminal(&workspace, window, cx);
            } else {
                self.open_workspace_and_create_entry(&key, NewEntryTarget::Terminal, window, cx);
            }
        } else if let Some(workspace) = self.active_workspace(cx) {
            self.create_new_terminal(&workspace, window, cx);
        }
    }
}
