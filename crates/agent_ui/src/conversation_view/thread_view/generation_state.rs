use super::*;

impl ThreadView {
    pub(crate) fn sync_editor_mode_for_empty_state(&mut self, cx: &mut Context<Self>) {
        let has_messages = self.list_state.item_count() > 0;
        let full_height_empty_state = !has_messages && !self.is_draft(cx);

        let mode = if full_height_empty_state {
            EditorMode::Full {
                scale_ui_elements_with_buffer_font_size: false,
                show_active_line_background: false,
                sizing_behavior: SizingBehavior::Default,
            }
        } else {
            EditorMode::AutoHeight {
                min_lines: AgentSettings::get_global(cx).message_editor_min_lines,
                max_lines: Some(AgentSettings::get_global(cx).set_message_editor_max_lines()),
            }
        };
        self.message_editor.update(cx, |editor, cx| {
            editor.set_mode(mode, cx);
        });
    }

    /// Ensures the list item count includes (or excludes) an extra item for the generating indicator
    pub(crate) fn sync_generating_indicator(&mut self, cx: &App) {
        let thread = self.thread.read(cx);

        let is_generating =
            matches!(thread.status(), ThreadStatus::Generating) && !thread.is_compacting();

        if is_generating && !self.generating_indicator_in_list {
            let entries_count = self.thread.read(cx).entries().len();
            self.list_state.splice(entries_count..entries_count, 1);
            self.generating_indicator_in_list = true;
        } else if !is_generating && self.generating_indicator_in_list {
            let entries_count = self.thread.read(cx).entries().len();
            self.list_state.splice(entries_count..entries_count + 1, 0);
            self.generating_indicator_in_list = false;
        }
    }

    pub(super) fn render_generating(&self, confirmation: bool, cx: &App) -> impl IntoElement {
        let show_stats = AgentSettings::get_global(cx).show_turn_stats;
        let elapsed_label = show_stats
            .then(|| {
                self.turn_fields.turn_started_at.and_then(|started_at| {
                    let elapsed = started_at.elapsed();
                    (elapsed > STOPWATCH_THRESHOLD).then(|| duration_alt_display(elapsed))
                })
            })
            .flatten();

        let is_blocked_on_terminal_command =
            !confirmation && self.is_blocked_on_terminal_command(cx);
        let is_waiting = confirmation || self.thread.read(cx).has_in_progress_tool_calls();

        let turn_tokens_label = elapsed_label
            .is_some()
            .then(|| {
                self.turn_fields
                    .turn_tokens
                    .filter(|&tokens| tokens > TOKEN_THRESHOLD)
                    .map(|tokens| crate::humanize_token_count(tokens))
            })
            .flatten();

        let arrow_icon = if is_waiting {
            IconName::ArrowUp
        } else {
            IconName::ArrowDown
        };

        h_flex()
            .id("generating-spinner")
            .py_2()
            .px(rems_from_px(22.))
            .gap_2()
            .map(|this| {
                if confirmation {
                    this.child(
                        h_flex()
                            .w_2()
                            .justify_center()
                            .child(GeneratingSpinnerElement::new(SpinnerVariant::Sand)),
                    )
                    .child(
                        div().min_w(rems(8.)).child(
                            LoadingLabel::new("Awaiting Confirmation")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                } else if is_blocked_on_terminal_command {
                    this
                } else {
                    this.child(
                        h_flex()
                            .w_2()
                            .justify_center()
                            .child(GeneratingSpinnerElement::new(SpinnerVariant::Dots)),
                    )
                }
            })
            .when_some(elapsed_label, |this, elapsed| {
                this.child(
                    Label::new(elapsed)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .when_some(turn_tokens_label, |this, tokens| {
                this.child(
                    h_flex()
                        .gap_0p5()
                        .child(
                            Icon::new(arrow_icon)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format!("{} tokens", tokens))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn auto_expand_streaming_thought(&mut self, cx: &mut Context<Self>) {
        let thread = self.thread.clone();
        let changed = self.entry_view_state.update(cx, |state, cx| {
            let thread = thread.read(cx);
            if thread.status() != ThreadStatus::Generating {
                return false;
            }
            state.auto_expand_streaming_thought(thread, cx)
        });
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn clear_auto_expand_tracking(&mut self, cx: &mut Context<Self>) {
        self.entry_view_state.update(cx, |state, _cx| {
            state.clear_auto_expand_tracking();
        });
    }
}
