use super::*;

impl<T: 'static> Render for PromptEditor<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font_size = ThemeSettings::get_global(cx).ui_font_size(cx);
        let mut buttons = Vec::new();

        const RIGHT_PADDING: Pixels = px(9.);

        let (left_gutter_width, right_padding, explanation) = match &self.mode {
            PromptEditorMode::Buffer {
                id: _,
                codegen,
                editor_margins,
            } => {
                let codegen = codegen.read(cx);

                if codegen.alternative_count(cx) > 1 {
                    buttons.push(self.render_cycle_controls(codegen, cx));
                }

                let editor_margins = editor_margins.lock();
                let gutter = editor_margins.gutter;

                let left_gutter_width = gutter.full_width() + (gutter.margin / 2.0);
                let right_padding = editor_margins.right + RIGHT_PADDING;

                let active_alternative = codegen.active_alternative().read(cx);
                let explanation = active_alternative
                    .description
                    .clone()
                    .or_else(|| active_alternative.failure.clone());

                (left_gutter_width, right_padding, explanation)
            }
            PromptEditorMode::Terminal { .. } => {
                // Give the equivalent of the same left-padding that we're using on the right
                (Pixels::from(40.0), Pixels::from(24.), None)
            }
        };

        let bottom_padding = match &self.mode {
            PromptEditorMode::Buffer { .. } => rems_from_px(2.0),
            PromptEditorMode::Terminal { .. } => rems_from_px(4.0),
        };

        buttons.extend(self.render_buttons(window, cx));

        let menu_visible = self.is_completions_menu_visible(cx);
        let add_context_button = IconButton::new("add-context", IconName::AtSign)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .when(!menu_visible, |this| {
                this.tooltip(move |_window, cx| {
                    Tooltip::with_meta("Add Context", None, "Or type @ to include context", cx)
                })
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.trigger_completion_menu(window, cx);
            }));

        let markdown = window.use_state(cx, |_, cx| Markdown::new("".into(), None, None, cx));

        if let Some(explanation) = &explanation {
            markdown.update(cx, |markdown, cx| {
                markdown.reset(SharedString::from(explanation), cx);
            });
        }

        let explanation_label = self
            .render_markdown(markdown, markdown_style(window, cx))
            .into_any_element();

        v_flex()
            .key_context("InlineAssistant")
            .capture_action(cx.listener(Self::paste))
            .block_mouse_except_scroll()
            .size_full()
            .pt_0p5()
            .pb(bottom_padding)
            .pr(right_padding)
            .gap_0p5()
            .justify_center()
            .border_y_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().editor_background)
            .child(
                h_flex()
                    .on_action(cx.listener(Self::confirm))
                    .on_action(cx.listener(Self::secondary_confirm))
                    .on_action(cx.listener(Self::cancel))
                    .on_action(cx.listener(Self::move_up))
                    .on_action(cx.listener(Self::move_down))
                    .on_action(cx.listener(Self::thumbs_up))
                    .on_action(cx.listener(Self::thumbs_down))
                    .capture_action(cx.listener(Self::cycle_prev))
                    .capture_action(cx.listener(Self::cycle_next))
                    .on_action(cx.listener(|this, _: &ToggleModelSelector, window, cx| {
                        this.model_selector
                            .update(cx, |model_selector, cx| model_selector.toggle(window, cx));
                    }))
                    .on_action(cx.listener(|this, _: &CycleFavoriteModels, window, cx| {
                        this.model_selector.update(cx, |model_selector, cx| {
                            model_selector.cycle_favorite_models(window, cx);
                        });
                    }))
                    .child(
                        WithRemSize::new(ui_font_size)
                            .h_full()
                            .w(left_gutter_width)
                            .flex()
                            .flex_row()
                            .flex_shrink_0()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .child(self.render_close_button(cx))
                            .map(|el| {
                                let CodegenStatus::Error(error) = self.codegen_status(cx) else {
                                    return el;
                                };

                                let error_message = SharedString::from(error.to_string());
                                el.child(
                                    div()
                                        .id("error")
                                        .tooltip(Tooltip::text(error_message))
                                        .child(
                                            Icon::new(IconName::XCircle)
                                                .size(IconSize::Small)
                                                .color(Color::Error),
                                        ),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .child(div().flex_1().child(self.render_editor(window, cx)))
                            .child(
                                WithRemSize::new(ui_font_size)
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(add_context_button)
                                    .child(self.model_selector.clone())
                                    .children(buttons),
                            ),
                    ),
            )
            .when_some(explanation, |this, _| {
                this.child(
                    h_flex()
                        .size_full()
                        .justify_center()
                        .child(div().w(left_gutter_width + px(6.)))
                        .child(
                            div()
                                .size_full()
                                .min_w_0()
                                .pt(rems_from_px(3.))
                                .pl_0p5()
                                .flex_1()
                                .border_t_1()
                                .border_color(cx.theme().colors().border_variant)
                                .child(explanation_label),
                        ),
                )
            })
    }
}

fn markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let theme_settings = ThemeSettings::get_global(cx);
    let colors = cx.theme().colors();
    let mut text_style = window.text_style();

    text_style.refine(&TextStyleRefinement {
        font_family: Some(theme_settings.ui_font.family.clone()),
        color: Some(colors.text),
        ..Default::default()
    });

    MarkdownStyle {
        base_text_style: text_style.clone(),
        syntax: cx.theme().syntax().clone(),
        selection_background_color: colors.element_selection_background,
        heading_level_styles: Some(HeadingLevelStyles {
            h1: Some(TextStyleRefinement {
                font_size: Some(rems(1.15).into()),
                ..Default::default()
            }),
            h2: Some(TextStyleRefinement {
                font_size: Some(rems(1.1).into()),
                ..Default::default()
            }),
            h3: Some(TextStyleRefinement {
                font_size: Some(rems(1.05).into()),
                ..Default::default()
            }),
            h4: Some(TextStyleRefinement {
                font_size: Some(rems(1.).into()),
                ..Default::default()
            }),
            h5: Some(TextStyleRefinement {
                font_size: Some(rems(0.95).into()),
                ..Default::default()
            }),
            h6: Some(TextStyleRefinement {
                font_size: Some(rems(0.875).into()),
                ..Default::default()
            }),
        }),
        inline_code: TextStyleRefinement {
            font_family: Some(theme_settings.buffer_font.family.clone()),
            font_fallbacks: theme_settings.buffer_font.fallbacks.clone(),
            font_features: Some(theme_settings.buffer_font.features.clone()),
            background_color: Some(colors.editor_foreground.opacity(0.08)),
            ..Default::default()
        },
        ..Default::default()
    }
}
