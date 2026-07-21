use super::*;

impl ThreadView {
    pub(super) fn render_plan_summary(
        &self,
        plan: &Plan,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let plan_expanded = self.plan_expanded;
        let stats = plan.stats();

        let title = if let Some(entry) = stats.in_progress_entry
            && !plan_expanded
        {
            h_flex()
                .cursor_default()
                .relative()
                .w_full()
                .gap_1()
                .truncate()
                .child(
                    Label::new("Current:")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .line_clamp(1)
                        .child(MarkdownElement::new(
                            entry.content.clone(),
                            plan_label_markdown_style(&entry.status, window, cx),
                        )),
                )
                .when(stats.pending > 0, |this| {
                    this.child(
                        h_flex()
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .child(div().min_w_8().h_full().bg(linear_gradient(
                                90.,
                                linear_color_stop(self.activity_bar_bg(cx), 1.),
                                linear_color_stop(self.activity_bar_bg(cx).opacity(0.2), 0.),
                            )))
                            .child(
                                div().pr_0p5().bg(self.activity_bar_bg(cx)).child(
                                    Label::new(format!("{} left", stats.pending))
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                            ),
                    )
                })
        } else {
            let status_label = if stats.pending == 0 {
                "All Done".to_string()
            } else if stats.completed == 0 {
                format!("{} Tasks", plan.entries.len())
            } else {
                format!("{}/{}", stats.completed, plan.entries.len())
            };

            h_flex()
                .w_full()
                .gap_1()
                .justify_between()
                .child(
                    Label::new("Plan")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(status_label)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .mr_1(),
                )
        };

        h_flex()
            .id("plan_summary")
            .p_1()
            .w_full()
            .gap_1()
            .when(plan_expanded, |this| {
                this.border_b_1().border_color(cx.theme().colors().border)
            })
            .child(Disclosure::new("plan_disclosure", plan_expanded))
            .child(title.flex_1())
            .child(
                IconButton::new("dismiss-plan", IconName::Close)
                    .icon_size(IconSize::XSmall)
                    .shape(ui::IconButtonShape::Square)
                    .tooltip(Tooltip::text("Clear Plan"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.thread.update(cx, |thread, cx| thread.clear_plan(cx));
                        cx.stop_propagation();
                    })),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.plan_expanded = !this.plan_expanded;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn render_plan_entries(
        &self,
        plan: &Plan,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .id("plan_items_list")
            .max_h_40()
            .overflow_y_scroll()
            .child(
                v_flex().children(plan.entries.iter().enumerate().flat_map(|(index, entry)| {
                    let entry_bg = cx.theme().colors().editor_background;
                    let tooltip_text: SharedString =
                        entry.content.read(cx).source().to_string().into();

                    Some(
                        h_flex()
                            .id(("plan_entry_row", index))
                            .py_1()
                            .px_2()
                            .gap_2()
                            .justify_between()
                            .relative()
                            .bg(entry_bg)
                            .when(index < plan.entries.len() - 1, |parent| {
                                parent.border_color(cx.theme().colors().border).border_b_1()
                            })
                            .overflow_hidden()
                            .child(
                                h_flex()
                                    .id(("plan_entry", index))
                                    .gap_1p5()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(match entry.status {
                                        acp::PlanEntryStatus::InProgress => {
                                            Icon::new(IconName::TodoProgress)
                                                .size(IconSize::Small)
                                                .color(Color::Accent)
                                                .with_rotate_animation(2)
                                                .into_any_element()
                                        }
                                        acp::PlanEntryStatus::Completed => {
                                            Icon::new(IconName::TodoComplete)
                                                .size(IconSize::Small)
                                                .color(Color::Success)
                                                .into_any_element()
                                        }
                                        acp::PlanEntryStatus::Pending | _ => {
                                            Icon::new(IconName::TodoPending)
                                                .size(IconSize::Small)
                                                .color(Color::Muted)
                                                .into_any_element()
                                        }
                                    })
                                    .child(MarkdownElement::new(
                                        entry.content.clone(),
                                        plan_label_markdown_style(&entry.status, window, cx),
                                    )),
                            )
                            .child(div().absolute().top_0().right_0().h_full().w_8().bg(
                                linear_gradient(
                                    90.,
                                    linear_color_stop(entry_bg, 1.),
                                    linear_color_stop(entry_bg.opacity(0.), 0.),
                                ),
                            ))
                            .tooltip(Tooltip::text(tooltip_text)),
                    )
                })),
            )
            .into_any_element()
    }

    pub(super) fn render_completed_plan(
        &self,
        entries: &[PlanEntry],
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        v_flex()
            .px_5()
            .py_1p5()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .border_color(self.tool_card_border_color(cx))
                    .child(
                        h_flex()
                            .px_2()
                            .py_1()
                            .gap_1()
                            .bg(self.tool_card_header_bg(cx))
                            .border_b_1()
                            .border_color(self.tool_card_border_color(cx))
                            .child(
                                Label::new("Completed Plan")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(format!(
                                    "— {} {}",
                                    entries.len(),
                                    if entries.len() == 1 { "step" } else { "steps" }
                                ))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            ),
                    )
                    .child(
                        v_flex().children(entries.iter().enumerate().map(|(index, entry)| {
                            h_flex()
                                .py_1()
                                .px_2()
                                .gap_1p5()
                                .when(index < entries.len() - 1, |this| {
                                    this.border_b_1().border_color(cx.theme().colors().border)
                                })
                                .child(
                                    Icon::new(IconName::TodoComplete)
                                        .size(IconSize::Small)
                                        .color(Color::Success),
                                )
                                .child(
                                    div()
                                        .max_w_full()
                                        .overflow_x_hidden()
                                        .text_xs()
                                        .text_color(cx.theme().colors().text_muted)
                                        .child(MarkdownElement::new(
                                            entry.content.clone(),
                                            default_markdown_style(window, cx),
                                        )),
                                )
                        })),
                    ),
            )
            .into_any()
    }
}
