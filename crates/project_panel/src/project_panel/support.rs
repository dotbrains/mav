use super::*;

#[derive(Clone)]
pub(super) struct StickyProjectPanelCandidate {
    pub(super) index: usize,
    pub(super) depth: usize,
}

impl StickyCandidate for StickyProjectPanelCandidate {
    fn depth(&self) -> usize {
        self.depth
    }
}

pub(super) fn item_width_estimate(depth: usize, item_text_chars: usize, is_symlink: bool) -> usize {
    const ICON_SIZE_FACTOR: usize = 2;
    let mut item_width = depth * ICON_SIZE_FACTOR + item_text_chars;
    if is_symlink {
        item_width += ICON_SIZE_FACTOR;
    }
    item_width
}

impl Render for DraggedProjectEntryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = ThemeSettings::get_global(cx).ui_font.clone();
        h_flex()
            .font(ui_font)
            .pl(self.click_offset.x + px(12.))
            .pt(self.click_offset.y + px(12.))
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .py_1()
                    .px_2()
                    .rounded_lg()
                    .bg(cx.theme().colors().background)
                    .map(|this| {
                        if self.selections.len() > 1 && self.selections.contains(&self.selection) {
                            this.child(Label::new(format!("{} entries", self.selections.len())))
                        } else {
                            this.child(if let Some(icon) = &self.icon {
                                div().child(Icon::from_path(icon.clone()))
                            } else {
                                div()
                            })
                            .child(Label::new(self.filename.clone()))
                        }
                    }),
            )
    }
}

impl EventEmitter<Event> for ProjectPanel {}

impl EventEmitter<PanelEvent> for ProjectPanel {}

impl Panel for ProjectPanel {
    fn position(&self, _: &Window, cx: &App) -> DockPosition {
        match ProjectPanelSettings::get_global(cx).dock {
            DockSide::Left => DockPosition::Left,
            DockSide::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            let dock = match position {
                DockPosition::Left | DockPosition::Bottom => DockSide::Left,
                DockPosition::Right => DockSide::Right,
            };
            settings.project_panel.get_or_insert_default().dock = Some(dock);
        });
    }

    fn default_size(&self, _: &Window, cx: &App) -> Pixels {
        ProjectPanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _: &Window, _cx: &App) -> Option<IconName> {
        Some(IconName::FileTree)
    }

    fn button_visible(&self, cx: &App) -> bool {
        ProjectPanelSettings::get_global(cx).button
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Project Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn persistent_name() -> &'static str {
        "Project Panel"
    }

    fn panel_key() -> &'static str {
        PROJECT_PANEL_KEY
    }

    fn starts_open(&self, _: &Window, cx: &App) -> bool {
        if !ProjectPanelSettings::get_global(cx).starts_open {
            return false;
        }

        let project = &self.project.read(cx);
        project.visible_worktrees(cx).any(|tree| {
            tree.read(cx)
                .root_entry()
                .is_some_and(|entry| entry.is_dir())
        })
    }

    fn activation_priority(&self) -> u32 {
        1
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.project_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

impl ProjectPanel {
    pub fn select_path_for_test(&mut self, project_path: ProjectPath, cx: &App) {
        let Some(worktree) = self
            .project
            .read(cx)
            .worktree_for_id(project_path.worktree_id, cx)
        else {
            return;
        };
        let Some(entry) = worktree.read(cx).entry_for_path(project_path.path.as_ref()) else {
            return;
        };
        self.selection = Some(SelectedEntry {
            worktree_id: project_path.worktree_id,
            entry_id: entry.id,
        });
    }
}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ClipboardEntry {
    pub(super) fn is_cut(&self) -> bool {
        matches!(self, Self::Cut { .. })
    }

    pub(super) fn items(&self) -> &BTreeSet<SelectedEntry> {
        match self {
            ClipboardEntry::Copied(entries) | ClipboardEntry::Cut(entries) => entries,
        }
    }

    pub(super) fn into_copy_entry(self) -> Self {
        match self {
            ClipboardEntry::Copied(_) => self,
            ClipboardEntry::Cut(entries) => ClipboardEntry::Copied(entries),
        }
    }
}

#[inline]
fn cmp_worktree_entries(
    a: &Entry,
    b: &Entry,
    mode: &settings::ProjectPanelSortMode,
    order: &settings::ProjectPanelSortOrder,
) -> cmp::Ordering {
    let a = (&*a.path, a.is_file());
    let b = (&*b.path, b.is_file());
    util::paths::compare_rel_paths_by(a, b, (*mode).into(), (*order).into())
}

pub fn sort_worktree_entries(
    entries: &mut [impl AsRef<Entry>],
    mode: settings::ProjectPanelSortMode,
    order: settings::ProjectPanelSortOrder,
) {
    entries.sort_by(|lhs, rhs| cmp_worktree_entries(lhs.as_ref(), rhs.as_ref(), &mode, &order));
}

pub fn par_sort_worktree_entries(
    entries: &mut Vec<GitEntry>,
    mode: settings::ProjectPanelSortMode,
    order: settings::ProjectPanelSortOrder,
) {
    entries.par_sort_by(|lhs, rhs| cmp_worktree_entries(lhs, rhs, &mode, &order));
}

pub(super) fn git_status_indicator(git_status: GitSummary) -> Option<(&'static str, Color)> {
    if git_status.conflict > 0 {
        return Some(("!", Color::Conflict));
    }
    if git_status.untracked > 0 {
        return Some(("U", Color::Created));
    }
    if git_status.worktree.deleted > 0 {
        return Some(("D", Color::Deleted));
    }
    if git_status.worktree.modified > 0 {
        return Some(("M", Color::Modified));
    }
    if git_status.index.deleted > 0 {
        return Some(("D", Color::Deleted));
    }
    if git_status.index.modified > 0 {
        return Some(("M", Color::Modified));
    }
    if git_status.index.added > 0 {
        return Some(("A", Color::Created));
    }
    None
}
