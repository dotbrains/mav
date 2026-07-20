use super::*;

impl ThreadView {
    pub(super) fn render_tool_call_label(
        &self,
        entry_ix: usize,
        tool_call: &ToolCall,
        is_edit: bool,
        has_failed: bool,
        has_revealed_diff: bool,
        use_card_layout: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let has_location = tool_call.locations.len() == 1;
        let is_file = tool_call.kind == acp::ToolKind::Edit && has_location;
        let is_subagent_tool_call = tool_call.is_subagent();

        let file_icon = if has_location {
            FileIcons::get_icon(&tool_call.locations[0].path, cx)
                .map(|from_path| Icon::from_path(from_path).color(Color::Muted))
                .unwrap_or(Icon::new(IconName::ToolPencil).color(Color::Muted))
        } else {
            Icon::new(IconName::ToolPencil).color(Color::Muted)
        };

        let tool_icon = if is_file && has_failed && has_revealed_diff {
            div()
                .id(entry_ix)
                .tooltip(Tooltip::text("Interrupted Edit"))
                .child(DecoratedIcon::new(
                    file_icon,
                    Some(
                        IconDecoration::new(
                            IconDecorationKind::Triangle,
                            self.tool_card_header_bg(cx),
                            cx,
                        )
                        .color(cx.theme().status().warning)
                        .position(gpui::Point {
                            x: px(-2.),
                            y: px(-2.),
                        }),
                    ),
                ))
                .into_any_element()
        } else if is_file {
            div().child(file_icon).into_any_element()
        } else if is_subagent_tool_call {
            Icon::new(self.agent_icon)
                .size(IconSize::Small)
                .color(Color::Muted)
                .into_any_element()
        } else {
            Icon::new(match tool_call.kind {
                acp::ToolKind::Read => IconName::ToolSearch,
                acp::ToolKind::Edit => IconName::ToolPencil,
                acp::ToolKind::Delete => IconName::ToolDeleteFile,
                acp::ToolKind::Move => IconName::ArrowRightLeft,
                acp::ToolKind::Search => IconName::ToolSearch,
                acp::ToolKind::Execute => IconName::ToolTerminal,
                acp::ToolKind::Think => IconName::ToolThink,
                acp::ToolKind::Fetch => IconName::ToolWeb,
                acp::ToolKind::SwitchMode => IconName::ArrowRightLeft,
                acp::ToolKind::Other | _ => IconName::ToolHammer,
            })
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element()
        };

        let gradient_overlay = div()
            .absolute()
            .top_0()
            .right_0()
            .w_12()
            .h_full()
            .map(|this| {
                if use_card_layout {
                    this.bg(linear_gradient(
                        90.,
                        linear_color_stop(self.tool_card_header_bg(cx), 1.),
                        linear_color_stop(self.tool_card_header_bg(cx).opacity(0.2), 0.),
                    ))
                } else {
                    this.bg(linear_gradient(
                        90.,
                        linear_color_stop(cx.theme().colors().panel_background, 1.),
                        linear_color_stop(cx.theme().colors().panel_background.opacity(0.2), 0.),
                    ))
                }
            });

        h_flex()
            .relative()
            .w_full()
            .h(window.line_height() - px(2.))
            .text_size(self.tool_name_font_size())
            .gap_1p5()
            .when(has_location || use_card_layout, |this| this.px_1())
            .when(has_location, |this| {
                this.cursor(CursorStyle::PointingHand)
                    .rounded(rems_from_px(3.))
                    .hover(|s| s.bg(cx.theme().colors().element_hover.opacity(0.5)))
            })
            .overflow_hidden()
            .child(tool_icon)
            .child(if has_location {
                h_flex()
                    .id(("open-tool-call-location", entry_ix))
                    .w_full()
                    .map(|this| {
                        if use_card_layout {
                            this.text_color(cx.theme().colors().text)
                        } else {
                            this.text_color(cx.theme().colors().text_muted)
                        }
                    })
                    .child(
                        self.render_markdown(
                            tool_call.label.clone(),
                            MarkdownStyle {
                                prevent_mouse_interaction: true,
                                ..MarkdownStyle::themed(MarkdownFont::Agent, window, cx)
                                    .with_muted_text(cx)
                            },
                            cx,
                        ),
                    )
                    .tooltip(Tooltip::text("Go to File"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_tool_call_location(entry_ix, 0, window, cx);
                    }))
                    .into_any_element()
            } else {
                h_flex()
                    .w_full()
                    .child(self.render_markdown(
                        tool_call.label.clone(),
                        MarkdownStyle::themed(MarkdownFont::Agent, window, cx).with_muted_text(cx),
                        cx,
                    ))
                    .into_any()
            })
            .when(!is_edit, |this| this.child(gradient_overlay))
    }

    pub(super) fn open_tool_call_location(
        &self,
        entry_ix: usize,
        location_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let (tool_call_location, agent_location) = self
            .thread
            .read(cx)
            .entries()
            .get(entry_ix)?
            .location(location_ix)?;

        let project_path = self
            .project
            .upgrade()?
            .read(cx)
            .find_project_path(&tool_call_location.path, cx);

        let open_task = self
            .workspace
            .update(cx, |workspace, cx| {
                if let Some(project_path) = project_path {
                    workspace.open_path(project_path, None, true, window, cx)
                } else {
                    workspace.open_abs_path(
                        tool_call_location.path.clone(),
                        OpenOptions {
                            focus: Some(true),
                            ..Default::default()
                        },
                        window,
                        cx,
                    )
                }
            })
            .log_err()?;
        window
            .spawn(cx, async move |cx| {
                let item = open_task.await?;

                let Some(active_editor) = item.downcast::<Editor>() else {
                    return anyhow::Ok(());
                };

                active_editor.update_in(cx, |editor, window, cx| {
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    if snapshot.as_singleton().is_some()
                        && let Some(anchor) = snapshot.anchor_in_excerpt(agent_location.position)
                    {
                        editor.change_selections(Default::default(), window, cx, |selections| {
                            selections.select_anchor_ranges([anchor..anchor]);
                        })
                    } else {
                        let row = tool_call_location.line.unwrap_or_default();
                        editor.change_selections(Default::default(), window, cx, |selections| {
                            selections.select_ranges([Point::new(row, 0)..Point::new(row, 0)]);
                        })
                    }
                })?;

                anyhow::Ok(())
            })
            .detach_and_log_err(cx);

        None
    }
}
