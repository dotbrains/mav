use super::*;

pub(super) struct TokenUsageTooltip {
    pub(super) percentage: String,
    pub(super) used: String,
    pub(super) max: String,
    pub(super) input_tokens: String,
    pub(super) output_tokens: String,
    pub(super) input_max: String,
    pub(super) output_max: String,
    pub(super) show_split: bool,
    pub(super) cost_label: Option<String>,
    pub(super) separator_color: Color,
    pub(super) global_agents_md_loaded: bool,
    pub(super) project_rules_count: usize,
    pub(super) project_entry_ids: Vec<ProjectEntryId>,
    pub(super) workspace: WeakEntity<Workspace>,
}

impl Render for TokenUsageTooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let separator_color = self.separator_color;
        let percentage = self.percentage.clone();
        let used = self.used.clone();
        let max = self.max.clone();
        let input_tokens = self.input_tokens.clone();
        let output_tokens = self.output_tokens.clone();
        let input_max = self.input_max.clone();
        let output_max = self.output_max.clone();
        let show_split = self.show_split;
        let cost_label = self.cost_label.clone();
        let global_agents_md_loaded = self.global_agents_md_loaded;
        let project_rules_count = self.project_rules_count;
        let project_entry_ids = self.project_entry_ids.clone();
        let workspace = self.workspace.clone();

        ui::tooltip_container(cx, move |container, cx| {
            container
                .min_w_40()
                .child(
                    Label::new("Context")
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .when(!show_split, |this| {
                    this.child(
                        h_flex()
                            .gap_0p5()
                            .child(Label::new(percentage.clone()))
                            .child(Label::new("\u{2022}").color(separator_color).mx_1())
                            .child(Label::new(used.clone()))
                            .child(Label::new("/").color(separator_color))
                            .child(Label::new(max.clone()).color(Color::Muted)),
                    )
                })
                .when(show_split, |this| {
                    this.child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                h_flex()
                                    .gap_0p5()
                                    .child(Label::new("Input:").color(Color::Muted).mr_0p5())
                                    .child(Label::new(input_tokens))
                                    .child(Label::new("/").color(separator_color))
                                    .child(Label::new(input_max).color(Color::Muted)),
                            )
                            .child(
                                h_flex()
                                    .gap_0p5()
                                    .child(Label::new("Output:").color(Color::Muted).mr_0p5())
                                    .child(Label::new(output_tokens))
                                    .child(Label::new("/").color(separator_color))
                                    .child(Label::new(output_max).color(Color::Muted)),
                            ),
                    )
                })
                .when_some(cost_label, |this, cost_label| {
                    this.child(
                        v_flex()
                            .mt_1p5()
                            .pt_1p5()
                            .gap_0p5()
                            .border_t_1()
                            .border_color(cx.theme().colors().border_variant)
                            .child(
                                Label::new("Cost")
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                            .child(Label::new(cost_label)),
                    )
                })
                .when(
                    global_agents_md_loaded || project_rules_count > 0,
                    move |this| {
                        this.child(
                            v_flex()
                                .mt_1p5()
                                .pt_1p5()
                                .pb_0p5()
                                .gap_0p5()
                                .border_t_1()
                                .border_color(cx.theme().colors().border_variant)
                                .child(
                                    Label::new("Rules")
                                        .color(Color::Muted)
                                        .size(LabelSize::Small),
                                )
                                .child(
                                    v_flex()
                                        .mx_neg_1()
                                        .when(global_agents_md_loaded, {
                                            let workspace = workspace.clone();
                                            move |this| {
                                                this.child(
                                                    Button::new(
                                                        "open-global-agents-md",
                                                        "1 global rule",
                                                    )
                                                    .end_icon(
                                                        Icon::new(IconName::ArrowUpRight)
                                                            .color(Color::Muted)
                                                            .size(IconSize::XSmall),
                                                    )
                                                    .on_click(move |_, window, cx| {
                                                        workspace
                                                            .update(cx, |workspace, cx| {
                                                                workspace
                                                                    .open_abs_path(
                                                                        paths::agents_file()
                                                                            .clone(),
                                                                        workspace::OpenOptions {
                                                                            focus: Some(true),
                                                                            ..Default::default()
                                                                        },
                                                                        window,
                                                                        cx,
                                                                    )
                                                                    .detach_and_log_err(cx);
                                                            })
                                                            .log_err();
                                                    }),
                                                )
                                            }
                                        })
                                        .when(project_rules_count > 0, move |this| {
                                            let workspace = workspace.clone();
                                            let project_entry_ids = project_entry_ids.clone();
                                            this.child(
                                                Button::new(
                                                    "open-project-rules",
                                                    format!(
                                                        "{} project rules",
                                                        project_rules_count
                                                    ),
                                                )
                                                .end_icon(
                                                    Icon::new(IconName::ArrowUpRight)
                                                        .color(Color::Muted)
                                                        .size(IconSize::XSmall),
                                                )
                                                .on_click(move |_, window, cx| {
                                                    let _ =
                                                        workspace.update(cx, |workspace, cx| {
                                                            let project =
                                                                workspace.project().read(cx);
                                                            let paths = project_entry_ids
                                                                .iter()
                                                                .flat_map(|id| {
                                                                    project.path_for_entry(*id, cx)
                                                                })
                                                                .collect::<Vec<_>>();
                                                            for path in paths {
                                                                workspace
                                                                    .open_path(
                                                                        path, None, true, window,
                                                                        cx,
                                                                    )
                                                                    .detach_and_log_err(cx);
                                                            }
                                                        });
                                                }),
                                            )
                                        }),
                                ),
                        )
                    },
                )
        })
    }
}
