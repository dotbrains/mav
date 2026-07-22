use super::*;

impl Render for BreakpointList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl ui::IntoElement {
        let breakpoints = self.breakpoint_store.read(cx).all_source_breakpoints(cx);
        self.breakpoints.clear();
        let path_style = self.worktree_store.read(cx).path_style();
        let weak = cx.weak_entity();
        let breakpoints = breakpoints.into_iter().flat_map(|(path, mut breakpoints)| {
            let relative_worktree_path = self
                .worktree_store
                .read(cx)
                .find_worktree(&path, cx)
                .and_then(|(worktree, relative_path)| {
                    worktree
                        .read(cx)
                        .is_visible()
                        .then(|| worktree.read(cx).root_name().join(&relative_path))
                });
            breakpoints.sort_by_key(|breakpoint| breakpoint.row);
            let weak = weak.clone();
            breakpoints.into_iter().filter_map(move |breakpoint| {
                debug_assert_eq!(&path, &breakpoint.path);
                let file_name = breakpoint.path.file_name()?;
                let breakpoint_path = RelPath::new(&breakpoint.path, path_style).ok();

                let dir = relative_worktree_path
                    .as_deref()
                    .or(breakpoint_path.as_deref())?
                    .parent()
                    .map(|parent| SharedString::from(parent.display(path_style).to_string()));
                let name = file_name
                    .to_str()
                    .map(ToOwned::to_owned)
                    .map(SharedString::from)?;
                let weak = weak.clone();
                let line = breakpoint.row + 1;
                Some(BreakpointEntry {
                    kind: BreakpointEntryKind::LineBreakpoint(LineBreakpoint {
                        name,
                        dir,
                        line,
                        breakpoint,
                    }),
                    weak,
                })
            })
        });
        let exception_breakpoints = self.session.as_ref().into_iter().flat_map(|session| {
            session
                .read(cx)
                .exception_breakpoints()
                .map(|(data, is_enabled)| BreakpointEntry {
                    kind: BreakpointEntryKind::ExceptionBreakpoint(ExceptionBreakpoint {
                        id: data.filter.clone(),
                        data: data.clone(),
                        is_enabled: *is_enabled,
                    }),
                    weak: weak.clone(),
                })
        });
        let data_breakpoints = self.session.as_ref().into_iter().flat_map(|session| {
            session
                .read(cx)
                .data_breakpoints()
                .map(|state| BreakpointEntry {
                    kind: BreakpointEntryKind::DataBreakpoint(DataBreakpoint(state.clone())),
                    weak: weak.clone(),
                })
        });
        self.breakpoints.extend(
            breakpoints
                .chain(data_breakpoints)
                .chain(exception_breakpoints),
        );

        let text_pixels = ui::TextSize::Default.pixels(cx).to_f64() as f32;

        self.max_width_index = self
            .breakpoints
            .iter()
            .map(|entry| match &entry.kind {
                BreakpointEntryKind::LineBreakpoint(line_bp) => {
                    let name_and_line = format!("{}:{}", line_bp.name, line_bp.line);
                    let dir_len = line_bp.dir.as_ref().map(|d| d.len()).unwrap_or(0);
                    (name_and_line.len() + dir_len) as f32 * text_pixels
                }
                BreakpointEntryKind::ExceptionBreakpoint(exc_bp) => {
                    exc_bp.data.label.len() as f32 * text_pixels
                }
                BreakpointEntryKind::DataBreakpoint(data_bp) => {
                    data_bp.0.context.human_readable_label().len() as f32 * text_pixels
                }
            })
            .position_max_by(|left, right| left.total_cmp(right));

        v_flex()
            .id("breakpoint-list")
            .key_context("BreakpointList")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_enable_breakpoint))
            .on_action(cx.listener(Self::unset_breakpoint))
            .on_action(cx.listener(Self::next_breakpoint_property))
            .on_action(cx.listener(Self::previous_breakpoint_property))
            .size_full()
            .pt_1()
            .child(self.render_list(cx))
            .custom_scrollbars(
                ui::Scrollbars::new(ScrollAxes::Both)
                    .tracked_scroll_handle(&self.scroll_handle)
                    .with_track_along(ScrollAxes::Both, cx.theme().colors().panel_background)
                    .tracked_entity(cx.entity_id()),
                window,
                cx,
            )
            .when_some(self.strip_mode, |this, _| {
                this.child(Divider::horizontal().color(DividerColor::Border))
                    .child(
                        h_flex()
                            .p_1()
                            .rounded_sm()
                            .bg(cx.theme().colors().editor_background)
                            .border_1()
                            .when(
                                self.input.focus_handle(cx).contains_focused(window, cx),
                                |this| {
                                    let colors = cx.theme().colors();

                                    let border_color = if self.input.read(cx).read_only(cx) {
                                        colors.border_disabled
                                    } else {
                                        colors.border_transparent
                                    };

                                    this.border_color(border_color)
                                },
                            )
                            .child(self.input.clone()),
                    )
            })
    }
}
