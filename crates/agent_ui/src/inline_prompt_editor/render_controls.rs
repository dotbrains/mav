use super::*;

impl<T: 'static> PromptEditor<T> {
    pub(super) fn render_buttons(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mode = match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => {
                let codegen = codegen.read(cx);
                if codegen.is_insertion {
                    GenerationMode::Generate
                } else {
                    GenerationMode::Transform
                }
            }
            PromptEditorMode::Terminal { .. } => GenerationMode::Generate,
        };

        let codegen_status = self.codegen_status(cx);

        match codegen_status {
            CodegenStatus::Idle => {
                vec![
                    Button::new("start", mode.start_label())
                        .label_size(LabelSize::Small)
                        .end_icon(
                            Icon::new(IconName::Return)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .on_click(
                            cx.listener(|_, _, _, cx| cx.emit(PromptEditorEvent::StartRequested)),
                        )
                        .into_any_element(),
                ]
            }
            CodegenStatus::Pending => vec![
                IconButton::new("stop", IconName::Stop)
                    .icon_color(Color::Error)
                    .shape(IconButtonShape::Square)
                    .tooltip(move |_window, cx| {
                        Tooltip::with_meta(
                            mode.tooltip_interrupt(),
                            Some(&menu::Cancel),
                            "Changes won't be discarded",
                            cx,
                        )
                    })
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PromptEditorEvent::StopRequested)))
                    .into_any_element(),
            ],
            CodegenStatus::Done | CodegenStatus::Error(_) => {
                let has_error = matches!(codegen_status, CodegenStatus::Error(_));
                if has_error || self.edited_since_done {
                    vec![
                        IconButton::new("restart", IconName::RotateCw)
                            .icon_color(Color::Info)
                            .shape(IconButtonShape::Square)
                            .tooltip(move |_window, cx| {
                                Tooltip::with_meta(
                                    mode.tooltip_restart(),
                                    Some(&menu::Confirm),
                                    "Changes will be discarded",
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(PromptEditorEvent::StartRequested);
                            }))
                            .into_any_element(),
                    ]
                } else {
                    let rated = matches!(self.session_state.completion, CompletionState::Rated);

                    let accept = IconButton::new("accept", IconName::Check)
                        .icon_color(Color::Info)
                        .shape(IconButtonShape::Square)
                        .tooltip(move |_window, cx| {
                            Tooltip::for_action(mode.tooltip_accept(), &menu::Confirm, cx)
                        })
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(PromptEditorEvent::ConfirmRequested { execute: false });
                        }))
                        .into_any_element();

                    let mut buttons = Vec::new();

                    if AgentSettings::get_global(cx).enable_feedback {
                        buttons.push(
                            h_flex()
                                .pl_1()
                                .gap_1()
                                .border_l_1()
                                .border_color(cx.theme().colors().border_variant)
                                .child(
                                    IconButton::new("thumbs-up", IconName::ThumbsUp)
                                        .shape(IconButtonShape::Square)
                                        .map(|this| {
                                            if rated {
                                                this.disabled(true)
                                                    .icon_color(Color::Disabled)
                                                    .tooltip(move |_, cx| {
                                                        Tooltip::with_meta(
                                                            "Good Result",
                                                            None,
                                                            "You already rated this result",
                                                            cx,
                                                        )
                                                    })
                                            } else {
                                                this.icon_color(Color::Muted).tooltip(
                                                    move |_, cx| {
                                                        Tooltip::for_action(
                                                            "Good Result",
                                                            &ThumbsUpResult,
                                                            cx,
                                                        )
                                                    },
                                                )
                                            }
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.thumbs_up(&ThumbsUpResult, window, cx);
                                        })),
                                )
                                .child(
                                    IconButton::new("thumbs-down", IconName::ThumbsDown)
                                        .shape(IconButtonShape::Square)
                                        .map(|this| {
                                            if rated {
                                                this.disabled(true)
                                                    .icon_color(Color::Disabled)
                                                    .tooltip(move |_, cx| {
                                                        Tooltip::with_meta(
                                                            "Bad Result",
                                                            None,
                                                            "You already rated this result",
                                                            cx,
                                                        )
                                                    })
                                            } else {
                                                this.icon_color(Color::Muted).tooltip(
                                                    move |_, cx| {
                                                        Tooltip::for_action(
                                                            "Bad Result",
                                                            &ThumbsDownResult,
                                                            cx,
                                                        )
                                                    },
                                                )
                                            }
                                        })
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.thumbs_down(&ThumbsDownResult, window, cx);
                                        })),
                                )
                                .into_any_element(),
                        );
                    }

                    buttons.push(accept);

                    match &self.mode {
                        PromptEditorMode::Terminal { .. } => {
                            buttons.push(
                                IconButton::new("confirm", IconName::PlayFilled)
                                    .icon_color(Color::Info)
                                    .shape(IconButtonShape::Square)
                                    .tooltip(|_window, cx| {
                                        Tooltip::for_action(
                                            "Execute Generated Command",
                                            &menu::SecondaryConfirm,
                                            cx,
                                        )
                                    })
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.emit(PromptEditorEvent::ConfirmRequested {
                                            execute: true,
                                        });
                                    }))
                                    .into_any_element(),
                            );
                            buttons
                        }
                        PromptEditorMode::Buffer { .. } => buttons,
                    }
                }
            }
        }
    }

    pub(super) fn cycle_prev(
        &mut self,
        _: &CyclePreviousInlineAssist,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => {
                codegen.update(cx, |codegen, cx| codegen.cycle_prev(cx));
            }
            PromptEditorMode::Terminal { .. } => {
                // no cycle buttons in terminal mode
            }
        }
    }

    pub(super) fn cycle_next(
        &mut self,
        _: &CycleNextInlineAssist,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => {
                codegen.update(cx, |codegen, cx| codegen.cycle_next(cx));
            }
            PromptEditorMode::Terminal { .. } => {
                // no cycle buttons in terminal mode
            }
        }
    }

    pub(super) fn render_close_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let focus_handle = self.editor.focus_handle(cx);

        IconButton::new("cancel", IconName::Close)
            .icon_color(Color::Muted)
            .shape(IconButtonShape::Square)
            .tooltip({
                move |_window, cx| {
                    Tooltip::for_action_in(
                        "Close Assistant",
                        &editor::actions::Cancel,
                        &focus_handle,
                        cx,
                    )
                }
            })
            .on_click(cx.listener(|_, _, _, cx| cx.emit(PromptEditorEvent::CancelRequested)))
            .into_any_element()
    }

    pub(super) fn render_cycle_controls(
        &self,
        codegen: &BufferCodegen,
        cx: &Context<Self>,
    ) -> AnyElement {
        let disabled = matches!(codegen.status(cx), CodegenStatus::Idle);

        let model_registry = LanguageModelRegistry::read_global(cx);
        let default_model = model_registry.default_model().map(|default| default.model);
        let alternative_models = model_registry.inline_alternative_models();

        let get_model_name = |index: usize| -> String {
            let name = |model: &Arc<dyn LanguageModel>| model.name().0.to_string();

            match index {
                0 => default_model.as_ref().map_or_else(String::new, name),
                index if index <= alternative_models.len() => alternative_models
                    .get(index - 1)
                    .map_or_else(String::new, name),
                _ => String::new(),
            }
        };

        let total_models = alternative_models.len() + 1;

        if total_models <= 1 {
            return div().into_any_element();
        }

        let current_index = codegen.active_alternative;
        let prev_index = (current_index + total_models - 1) % total_models;
        let next_index = (current_index + 1) % total_models;

        let prev_model_name = get_model_name(prev_index);
        let next_model_name = get_model_name(next_index);

        h_flex()
            .child(
                IconButton::new("previous", IconName::ChevronLeft)
                    .icon_color(Color::Muted)
                    .disabled(disabled || current_index == 0)
                    .shape(IconButtonShape::Square)
                    .tooltip({
                        let focus_handle = self.editor.focus_handle(cx);
                        move |_window, cx| {
                            cx.new(|cx| {
                                let mut tooltip = Tooltip::new("Previous Alternative").key_binding(
                                    KeyBinding::for_action_in(
                                        &CyclePreviousInlineAssist,
                                        &focus_handle,
                                        cx,
                                    ),
                                );
                                if !disabled && current_index != 0 {
                                    tooltip = tooltip.meta(prev_model_name.clone());
                                }
                                tooltip
                            })
                            .into()
                        }
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cycle_prev(&CyclePreviousInlineAssist, window, cx);
                    })),
            )
            .child(
                Label::new(format!(
                    "{}/{}",
                    codegen.active_alternative + 1,
                    codegen.alternative_count(cx)
                ))
                .size(LabelSize::Small)
                .color(if disabled {
                    Color::Disabled
                } else {
                    Color::Muted
                }),
            )
            .child(
                IconButton::new("next", IconName::ChevronRight)
                    .icon_color(Color::Muted)
                    .disabled(disabled || current_index == total_models - 1)
                    .shape(IconButtonShape::Square)
                    .tooltip({
                        let focus_handle = self.editor.focus_handle(cx);
                        move |_window, cx| {
                            cx.new(|cx| {
                                let mut tooltip = Tooltip::new("Next Alternative").key_binding(
                                    KeyBinding::for_action_in(
                                        &CycleNextInlineAssist,
                                        &focus_handle,
                                        cx,
                                    ),
                                );
                                if !disabled && current_index != total_models - 1 {
                                    tooltip = tooltip.meta(next_model_name.clone());
                                }
                                tooltip
                            })
                            .into()
                        }
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cycle_next(&CycleNextInlineAssist, window, cx)
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_editor(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();

        div()
            .size_full()
            .p_2()
            .pl_1()
            .bg(colors.editor_background)
            .child({
                let settings = ThemeSettings::get_global(cx);
                let font_size = settings.buffer_font_size(cx);

                let text_style = TextStyle {
                    color: colors.editor_foreground,
                    font_family: settings.buffer_font.family.clone(),
                    font_features: settings.buffer_font.features.clone(),
                    font_size: font_size.into(),
                    line_height: relative(settings.buffer_line_height.value()),
                    ..Default::default()
                };

                EditorElement::new(
                    &self.editor,
                    EditorStyle {
                        background: colors.editor_background,
                        local_player: cx.theme().players().local(),
                        syntax: cx.theme().syntax().clone(),
                        text: text_style,
                        ..Default::default()
                    },
                )
            })
            .into_any_element()
    }

    pub(super) fn render_markdown(
        &self,
        markdown: Entity<Markdown>,
        style: MarkdownStyle,
    ) -> MarkdownElement {
        MarkdownElement::new(markdown, style)
            .image_resolver(|dest_url| crate::resolve_agent_image(dest_url, &[]))
    }
}
