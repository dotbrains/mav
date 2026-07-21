use super::*;

impl ProjectPanel {
    pub(super) fn rendered_entry_id(entry_id: ProjectEntryId, is_sticky: bool) -> ElementId {
        if is_sticky {
            SharedString::from(format!("project_panel_sticky_item_{}", entry_id.to_usize())).into()
        } else {
            (entry_id.to_proto() as usize).into()
        }
    }

    pub(super) fn rendered_entry_git_indicator(
        settings: &ProjectPanelSettings,
        git_status: GitSummary,
    ) -> Option<(&'static str, Color)> {
        settings
            .git_status_indicator
            .then(|| git_status_indicator(git_status))
            .flatten()
    }

    pub(super) fn rendered_entry_border_color(
        &self,
        is_active: bool,
        default_color: Hsla,
        focused_color: Hsla,
        validation_color_and_message: Option<(Hsla, String)>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Hsla {
        if !self.mouse_down && is_active && self.focus_handle.contains_focused(window, cx) {
            match validation_color_and_message {
                Some((color, _)) => color,
                None => focused_color,
            }
        } else {
            default_color
        }
    }

    pub(super) fn is_entry_highlighted(
        &self,
        entry_id: ProjectEntryId,
        worktree_id: WorktreeId,
        path: &RelPath,
        cx: &Context<Self>,
    ) -> bool {
        let Some(highlight_entry_id) =
            self.drag_target_entry
                .as_ref()
                .and_then(|drag_target| match drag_target {
                    DragTarget::Entry {
                        highlight_entry_id, ..
                    } => Some(*highlight_entry_id),
                    DragTarget::Background => self.state.last_worktree_root_id,
                })
        else {
            return false;
        };

        if entry_id == highlight_entry_id {
            return true;
        }

        maybe!({
            let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
            let highlight_entry = worktree.read(cx).entry_for_id(highlight_entry_id)?;
            Some(path.starts_with(&highlight_entry.path))
        })
        .unwrap_or(false)
    }

    pub(super) fn render_entry_end_slot(
        canonical_path: Option<Arc<Path>>,
        diagnostic_count: Option<DiagnosticCount>,
        git_indicator: Option<(&'static str, Color)>,
        kind: EntryKind,
        filename_text_color: Color,
        cx: &Context<Self>,
    ) -> AnyElement {
        let symlink_element = canonical_path.map(|path| {
            div()
                .id("symlink_icon")
                .tooltip(move |_window, cx| {
                    Tooltip::with_meta(
                        path.to_string_lossy().into_owned(),
                        None,
                        "Symbolic Link",
                        cx,
                    )
                })
                .child(
                    Icon::new(IconName::ArrowUpRight)
                        .size(IconSize::Indicator)
                        .color(filename_text_color),
                )
        });

        h_flex()
            .gap_1()
            .flex_none()
            .pr_3()
            .when_some(diagnostic_count, |this, count| {
                this.when(count.error_count > 0, |this| {
                    this.child(
                        Label::new(count.capped_error_count())
                            .size(LabelSize::Small)
                            .color(Color::Error),
                    )
                })
                .when(count.warning_count > 0, |this| {
                    this.child(
                        Label::new(count.capped_warning_count())
                            .size(LabelSize::Small)
                            .color(Color::Warning),
                    )
                })
            })
            .when_some(git_indicator, |this, (label, color)| {
                let git_indicator = if kind.is_dir() {
                    Indicator::dot()
                        .color(Color::Custom(color.color(cx).opacity(0.5)))
                        .into_any_element()
                } else {
                    Label::new(label)
                        .size(LabelSize::Small)
                        .color(color)
                        .into_any_element()
                };

                this.child(git_indicator)
            })
            .when_some(symlink_element, |this, el| this.child(el))
            .into_any_element()
    }
}
