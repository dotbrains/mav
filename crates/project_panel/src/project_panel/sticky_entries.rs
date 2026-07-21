use super::*;

impl ProjectPanel {
    pub(super) fn render_sticky_entries(
        &self,
        child: StickyProjectPanelCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> SmallVec<[AnyElement; 8]> {
        let project = self.project.read(cx);

        let Some((worktree_id, entry_ref)) = self.entry_at_index(child.index) else {
            return SmallVec::new();
        };

        let Some(visible) = self
            .state
            .visible_entries
            .iter()
            .find(|worktree| worktree.worktree_id == worktree_id)
        else {
            return SmallVec::new();
        };

        let Some(worktree) = project.worktree_for_id(worktree_id, cx) else {
            return SmallVec::new();
        };
        let worktree = worktree.read(cx).snapshot();

        let paths = visible
            .index
            .get_or_init(|| visible.entries.iter().map(|e| e.path.clone()).collect());

        let mut sticky_parents = Vec::new();
        let mut current_path = entry_ref.path.clone();

        'outer: loop {
            if let Some(parent_path) = current_path.parent() {
                for ancestor_path in parent_path.ancestors() {
                    if paths.contains(ancestor_path)
                        && let Some(parent_entry) = worktree.entry_for_path(ancestor_path)
                    {
                        sticky_parents.push(parent_entry.clone());
                        current_path = parent_entry.path.clone();
                        continue 'outer;
                    }
                }
            }
            break 'outer;
        }

        if sticky_parents.is_empty() {
            return SmallVec::new();
        }

        sticky_parents.reverse();

        let panel_settings = ProjectPanelSettings::get_global(cx);
        let git_status_enabled = panel_settings.git_status;
        let root_name = worktree.root_name();

        let git_summaries_by_id = if git_status_enabled {
            visible
                .entries
                .iter()
                .map(|e| (e.id, e.git_summary))
                .collect::<HashMap<_, _>>()
        } else {
            Default::default()
        };

        // already checked if non empty above
        let last_item_index = sticky_parents.len() - 1;
        let marked_selections: Arc<[SelectedEntry]> = Arc::from(self.marked_entries.clone());
        sticky_parents
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let git_status = git_summaries_by_id
                    .get(&entry.id)
                    .copied()
                    .unwrap_or_default();
                let sticky_details = Some(StickyDetails {
                    sticky_index: index,
                });
                let details = self.details_for_entry(
                    entry,
                    worktree_id,
                    root_name,
                    paths,
                    git_status,
                    sticky_details,
                    window,
                    cx,
                );
                self.render_entry(
                    entry.id,
                    details,
                    Arc::clone(&marked_selections),
                    window,
                    cx,
                )
                .when(index == last_item_index, |this| {
                    let shadow_color_top = hsla(0.0, 0.0, 0.0, 0.1);
                    let shadow_color_bottom = hsla(0.0, 0.0, 0.0, 0.);
                    let sticky_shadow = div()
                        .absolute()
                        .left_0()
                        .bottom_neg_1p5()
                        .h_1p5()
                        .w_full()
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(shadow_color_top, 1.),
                            linear_color_stop(shadow_color_bottom, 0.),
                        ));
                    this.child(sticky_shadow)
                })
                .into_any()
            })
            .collect()
    }
}
