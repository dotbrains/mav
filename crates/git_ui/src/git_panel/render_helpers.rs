use super::*;

impl GitPanel {
    pub(super) fn compute_visible_depths(&self, range: Range<usize>) -> SmallVec<[usize; 64]> {
        let GitPanelViewMode::Tree(state) = &self.view_mode else {
            return SmallVec::new();
        };

        range
            .map(|ix| {
                state
                    .logical_indices
                    .get(ix)
                    .and_then(|&entry_ix| self.entries.get(entry_ix))
                    .map_or(0, |entry| entry.depth())
            })
            .collect()
    }

    pub(super) fn status_width_estimate(
        tree_view: bool,
        entry: &GitStatusEntry,
        path_style: PathStyle,
        depth: usize,
    ) -> usize {
        if tree_view {
            Self::item_width_estimate(0, entry.display_name(path_style).len(), depth)
        } else {
            Self::item_width_estimate(
                entry.parent_dir(path_style).map(|s| s.len()).unwrap_or(0),
                entry.display_name(path_style).len(),
                0,
            )
        }
    }

    pub(super) fn width_estimate_for_list_entry(
        &self,
        tree_view: bool,
        entry: &GitListEntry,
        path_style: PathStyle,
    ) -> Option<usize> {
        match entry {
            GitListEntry::Status(status) => Some(Self::status_width_estimate(
                tree_view, status, path_style, 0,
            )),
            GitListEntry::TreeStatus(status) => Some(Self::status_width_estimate(
                tree_view,
                &status.entry,
                path_style,
                status.depth,
            )),
            GitListEntry::Directory(dir) => {
                Some(Self::item_width_estimate(0, dir.name.len(), dir.depth))
            }
            GitListEntry::Header(_) => None,
        }
    }

    pub(super) fn item_width_estimate(path: usize, file_name: usize, depth: usize) -> usize {
        path + file_name + depth * 2
    }

    pub(super) fn render_changes_header(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if matches!(self.git_access, GitAccess::No) {
            return None;
        }

        self.active_repository.as_ref()?;

        let diff_stat_total = self.diff_stat_total;

        Some(
            h_flex()
                .min_h(Tab::container_height(cx))
                .w_full()
                .pl_1()
                .pr_2()
                .flex_none()
                .flex_wrap()
                .gap_1()
                .justify_between()
                .child(
                    ButtonLike::new("diff-button")
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Icon::new(IconName::Diff)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(
                                    Label::new("View Diff")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .when(
                                    GitPanelSettings::get_global(cx).diff_stats
                                        && diff_stat_total != DiffStat::default(),
                                    |this| {
                                        this.child(ui::DiffStat::new(
                                            "changes-diff-stat-total",
                                            diff_stat_total.added as usize,
                                            diff_stat_total.deleted as usize,
                                        ))
                                    },
                                ),
                        )
                        .tooltip(Tooltip::for_action_title_in(
                            "View Diff",
                            &Diff,
                            &self.focus_handle,
                        ))
                        .on_click(|_, _, cx| {
                            cx.defer(|cx| {
                                cx.dispatch_action(&Diff);
                            })
                        }),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(self.render_view_options_menu("view_options_menu"))
                        .child(self.render_git_changes_actions_button(cx)),
                ),
        )
    }
}
