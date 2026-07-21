use super::token_usage_tooltip::TokenUsageTooltip;
use super::*;

impl ThreadView {
    fn supports_split_token_display(&self, cx: &App) -> bool {
        self.as_native_thread(cx)
            .and_then(|thread| thread.read(cx).model())
            .is_some_and(|model| model.supports_split_token_display())
    }

    pub(super) fn render_token_usage(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let thread = self.thread.read(cx);
        let usage = thread.token_usage()?;
        let show_split = self.supports_split_token_display(cx);

        let cost_label = thread.cost().map(|cost| {
            let precision = if cost.amount > 0.0 && cost.amount < 0.01 {
                4
            } else {
                2
            };
            format!("{:.prec$} {}", cost.amount, cost.currency, prec = precision)
        });

        let progress_color = |ratio: f32| -> Hsla {
            if ratio >= 0.85 {
                cx.theme().status().warning
            } else {
                cx.theme().colors().text_muted
            }
        };

        let used = crate::humanize_token_count(usage.used_tokens);
        let max = crate::humanize_token_count(usage.max_tokens);
        let input_tokens_label = crate::humanize_token_count(usage.input_tokens);
        let output_tokens_label = crate::humanize_token_count(usage.output_tokens);

        let progress_ratio = if usage.max_tokens > 0 {
            usage.used_tokens as f32 / usage.max_tokens as f32
        } else {
            0.0
        };

        let ring_size = px(16.0);
        let stroke_width = px(2.);

        let percentage = format!("{}%", (progress_ratio * 100.0).round() as u32);

        let tooltip_separator_color = Color::Custom(cx.theme().colors().text_disabled.opacity(0.6));

        let (project_rules_count, project_entry_ids) = self
            .as_native_thread(cx)
            .map(|thread| {
                let project_context = thread.read(cx).project_context().read(cx);
                let project_entry_ids = project_context
                    .worktrees
                    .iter()
                    .filter_map(|wt| wt.rules_file.as_ref())
                    .map(|rf| ProjectEntryId::from_usize(rf.project_entry_id))
                    .collect::<Vec<_>>();
                let project_rules_count = project_entry_ids.len();
                (project_rules_count, project_entry_ids)
            })
            .unwrap_or_default();

        let global_agents_md_loaded = UserAgentsMd::global(cx)
            .and_then(|md| md.content())
            .is_some();

        let workspace = self.workspace.clone();

        let max_output_tokens = self
            .as_native_thread(cx)
            .and_then(|thread| thread.read(cx).model())
            .and_then(|model| model.max_output_tokens())
            .unwrap_or(0);
        let input_max_label =
            crate::humanize_token_count(usage.max_tokens.saturating_sub(max_output_tokens));
        let output_max_label = crate::humanize_token_count(max_output_tokens);

        let build_tooltip = {
            move |_window: &mut Window, cx: &mut App| {
                let percentage = percentage.clone();
                let used = used.clone();
                let max = max.clone();
                let input_tokens_label = input_tokens_label.clone();
                let output_tokens_label = output_tokens_label.clone();
                let input_max_label = input_max_label.clone();
                let output_max_label = output_max_label.clone();
                let project_entry_ids = project_entry_ids.clone();
                let workspace = workspace.clone();
                let cost_label = cost_label.clone();
                cx.new(move |_cx| TokenUsageTooltip {
                    percentage,
                    used,
                    max,
                    input_tokens: input_tokens_label,
                    output_tokens: output_tokens_label,
                    input_max: input_max_label,
                    output_max: output_max_label,
                    show_split,
                    cost_label,
                    separator_color: tooltip_separator_color,
                    global_agents_md_loaded,
                    project_rules_count,
                    project_entry_ids,
                    workspace,
                })
                .into()
            }
        };

        if show_split {
            let input_max_raw = usage.max_tokens.saturating_sub(max_output_tokens);
            let output_max_raw = max_output_tokens;

            let input_ratio = if input_max_raw > 0 {
                usage.input_tokens as f32 / input_max_raw as f32
            } else {
                0.0
            };
            let output_ratio = if output_max_raw > 0 {
                usage.output_tokens as f32 / output_max_raw as f32
            } else {
                0.0
            };

            Some(
                h_flex()
                    .id("split_token_usage")
                    .flex_shrink_0()
                    .gap_1p5()
                    .mr_1()
                    .child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                Icon::new(IconName::ArrowUp)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                CircularProgress::new(
                                    usage.input_tokens as f32,
                                    input_max_raw as f32,
                                    ring_size,
                                    cx,
                                )
                                .stroke_width(stroke_width)
                                .progress_color(progress_color(input_ratio)),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                Icon::new(IconName::ArrowDown)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                CircularProgress::new(
                                    usage.output_tokens as f32,
                                    output_max_raw as f32,
                                    ring_size,
                                    cx,
                                )
                                .stroke_width(stroke_width)
                                .progress_color(progress_color(output_ratio)),
                            ),
                    )
                    .hoverable_tooltip(build_tooltip)
                    .into_any_element(),
            )
        } else {
            Some(
                h_flex()
                    .id("circular_progress_tokens")
                    .mt_px()
                    .mr_1()
                    .child(
                        CircularProgress::new(
                            usage.used_tokens as f32,
                            usage.max_tokens as f32,
                            ring_size,
                            cx,
                        )
                        .stroke_width(stroke_width)
                        .progress_color(progress_color(progress_ratio)),
                    )
                    .hoverable_tooltip(build_tooltip)
                    .into_any_element(),
            )
        }
    }
}
