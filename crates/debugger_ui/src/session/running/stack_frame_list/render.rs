use super::*;

impl StackFrameList {
    pub(super) fn render_label_entry(
        &self,
        stack_frame: &dap::StackFrame,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("label-stack-frame", stack_frame.id))
            .p_1()
            .on_any_mouse_down(|_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                v_flex().justify_center().gap_0p5().child(
                    Label::new(stack_frame.name.clone())
                        .size(LabelSize::Small)
                        .weight(FontWeight::BOLD)
                        .truncate()
                        .color(Color::Info),
                ),
            )
            .into_any()
    }

    pub(super) fn render_normal_entry(
        &self,
        ix: usize,
        stack_frame: &dap::StackFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = stack_frame.source.clone();
        let is_selected_frame = Some(ix) == self.selected_ix;

        let path = source.and_then(|s| s.path.or(s.name));
        let formatted_path = path.map(|path| format!("{}:{}", path, stack_frame.line,));
        let formatted_path = formatted_path.map(|path| {
            Label::new(path)
                .size(LabelSize::XSmall)
                .line_height_style(LineHeightStyle::UiLabel)
                .truncate()
                .color(Color::Muted)
        });

        let supports_frame_restart = self
            .session
            .read(cx)
            .capabilities()
            .supports_restart_frame
            .unwrap_or_default();

        let should_deemphasize = matches!(
            stack_frame.presentation_hint,
            Some(
                dap::StackFramePresentationHint::Subtle
                    | dap::StackFramePresentationHint::Deemphasize
            )
        );
        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("stack-frame", stack_frame.id))
            .p_1()
            .when(is_selected_frame, |this| {
                this.bg(cx.theme().colors().element_hover)
            })
            .on_any_mouse_down(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected_ix = Some(ix);
                this.activate_selected_entry(window, cx);
            }))
            .hover(|style| style.bg(cx.theme().colors().element_hover).cursor_pointer())
            .overflow_x_scroll()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        Label::new(stack_frame.name.clone())
                            .size(LabelSize::Small)
                            .truncate()
                            .when(should_deemphasize, |this| this.color(Color::Muted)),
                    )
                    .children(formatted_path),
            )
            .when(
                supports_frame_restart && stack_frame.can_restart.unwrap_or(true),
                |this| {
                    this.child(
                        h_flex()
                            .id(("restart-stack-frame", stack_frame.id))
                            .visible_on_hover("")
                            .absolute()
                            .right_2()
                            .overflow_hidden()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().colors().element_selected)
                            .bg(cx.theme().colors().element_background)
                            .hover(|style| {
                                style
                                    .bg(cx.theme().colors().ghost_element_hover)
                                    .cursor_pointer()
                            })
                            .child(
                                IconButton::new(
                                    ("restart-stack-frame", stack_frame.id),
                                    IconName::RotateCcw,
                                )
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener({
                                    let stack_frame_id = stack_frame.id;
                                    move |this, _, _window, cx| {
                                        this.restart_stack_frame(stack_frame_id, cx);
                                    }
                                }))
                                .tooltip(move |window, cx| {
                                    Tooltip::text("Restart Stack Frame")(window, cx)
                                }),
                            ),
                    )
                },
            )
            .into_any()
    }

    pub(super) fn render_collapsed_entry(
        &self,
        ix: usize,
        stack_frames: &Vec<dap::StackFrame>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first_stack_frame = &stack_frames[0];
        let is_selected = Some(ix) == self.selected_ix;

        h_flex()
            .rounded_md()
            .justify_between()
            .w_full()
            .group("")
            .id(("stack-frame", first_stack_frame.id))
            .p_1()
            .when(is_selected, |this| {
                this.bg(cx.theme().colors().element_hover)
            })
            .on_any_mouse_down(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected_ix = Some(ix);
                this.activate_selected_entry(window, cx);
            }))
            .hover(|style| style.bg(cx.theme().colors().element_hover).cursor_pointer())
            .child(
                v_flex()
                    .text_ui_sm(cx)
                    .truncate()
                    .text_color(cx.theme().colors().text_muted)
                    .child(format!(
                        "Show {} more{}",
                        stack_frames.len(),
                        first_stack_frame
                            .source
                            .as_ref()
                            .and_then(|source| source.origin.as_ref())
                            .map_or(String::new(), |origin| format!(": {}", origin))
                    )),
            )
            .into_any()
    }

    pub(super) fn render_entry(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let ix = match self.list_filter {
            StackFrameFilter::All => ix,
            StackFrameFilter::OnlyUserFrames => self.filter_entries_indices[ix],
        };

        match &self.entries[ix] {
            StackFrameEntry::Label(stack_frame) => self.render_label_entry(stack_frame, cx),
            StackFrameEntry::Normal(stack_frame) => self.render_normal_entry(ix, stack_frame, cx),
            StackFrameEntry::Collapsed(stack_frames) => {
                self.render_collapsed_entry(ix, stack_frames, cx)
            }
        }
    }

    fn select_ix(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.selected_ix = ix;
        cx.notify();
    }

    fn select_next(&mut self, _: &menu::SelectNext, _window: &mut Window, cx: &mut Context<Self>) {
        let ix = match self.selected_ix {
            _ if self.entries.is_empty() => None,
            None => Some(0),
            Some(ix) => {
                if ix == self.entries.len() - 1 {
                    Some(0)
                } else {
                    Some(ix + 1)
                }
            }
        };
        self.select_ix(ix, cx);
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = match self.selected_ix {
            _ if self.entries.is_empty() => None,
            None => Some(self.entries.len() - 1),
            Some(ix) => {
                if ix == 0 {
                    Some(self.entries.len() - 1)
                } else {
                    Some(ix - 1)
                }
            }
        };
        self.select_ix(ix, cx);
    }

    fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ix = if !self.entries.is_empty() {
            Some(0)
        } else {
            None
        };
        self.select_ix(ix, cx);
    }

    fn select_last(&mut self, _: &menu::SelectLast, _window: &mut Window, cx: &mut Context<Self>) {
        let ix = if !self.entries.is_empty() {
            Some(self.entries.len() - 1)
        } else {
            None
        };
        self.select_ix(ix, cx);
    }

    fn activate_selected_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.selected_ix else {
            return;
        };
        let Some(entry) = self.entries.get_mut(ix) else {
            return;
        };
        match entry {
            StackFrameEntry::Normal(stack_frame) => {
                let stack_frame = stack_frame.clone();
                self.go_to_stack_frame_inner(stack_frame, window, cx)
                    .detach_and_log_err(cx)
            }
            StackFrameEntry::Label(_) => {
                debug_panic!("You should not be able to select a label stack frame")
            }
            StackFrameEntry::Collapsed(_) => self.expand_collapsed_entry(ix, cx),
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_selected_entry(window, cx);
    }

    pub(crate) fn toggle_frame_filter(
        &mut self,
        thread_status: Option<ThreadStatus>,
        cx: &mut Context<Self>,
    ) {
        self.list_filter = match self.list_filter {
            StackFrameFilter::All => StackFrameFilter::OnlyUserFrames,
            StackFrameFilter::OnlyUserFrames => StackFrameFilter::All,
        };

        if let Some(database_id) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .ok()
            .flatten()
        {
            let key = stack_frame_filter_key(&self.session.read(cx).adapter(), database_id);
            let kvp = KeyValueStore::global(cx);
            let filter: String = self.list_filter.into();
            cx.background_spawn(async move { kvp.write_kvp(key, filter).await })
                .detach();
        }

        if let Some(ThreadStatus::Stopped) = thread_status {
            match self.list_filter {
                StackFrameFilter::All => {
                    self.list_state.reset(self.entries.len());
                }
                StackFrameFilter::OnlyUserFrames => {
                    self.list_state.reset(self.filter_entries_indices.len());
                    if !self
                        .selected_ix
                        .map(|ix| self.filter_entries_indices.contains(&ix))
                        .unwrap_or_default()
                    {
                        self.selected_ix = None;
                    }
                }
            }

            if let Some(ix) = self.selected_ix {
                let scroll_to = match self.list_filter {
                    StackFrameFilter::All => ix,
                    StackFrameFilter::OnlyUserFrames => self
                        .filter_entries_indices
                        .binary_search_by_key(&ix, |ix| *ix)
                        .expect("This index will always exist"),
                };
                self.list_state.scroll_to_reveal_item(scroll_to);
            }

            cx.emit(StackFrameListEvent::BuiltEntries);
            cx.notify();
        }
    }

    pub(super) fn render_list(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div().p_1().size_full().child(
            list(
                self.list_state.clone(),
                cx.processor(|this, ix, _window, cx| this.render_entry(ix, cx)),
            )
            .size_full(),
        )
    }

    pub(crate) fn render_control_strip(&self) -> AnyElement {
        let tooltip_title = match self.list_filter {
            StackFrameFilter::All => "Show stack frames from your project",
            StackFrameFilter::OnlyUserFrames => "Show all stack frames",
        };

        h_flex()
            .child(
                IconButton::new(
                    "filter-by-visible-worktree-stack-frame-list",
                    IconName::ListFilter,
                )
                .tooltip(move |_window, cx| {
                    Tooltip::for_action(tooltip_title, &ToggleUserFrames, cx)
                })
                .toggle_state(self.list_filter == StackFrameFilter::OnlyUserFrames)
                .icon_size(IconSize::Small)
                .on_click(|_, window, cx| {
                    window.dispatch_action(ToggleUserFrames.boxed_clone(), cx)
                }),
            )
            .into_any_element()
    }
}
