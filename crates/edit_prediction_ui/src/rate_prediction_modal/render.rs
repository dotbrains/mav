use super::*;

impl RatePredictionsModal {
    fn render_view_nav(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .h_8()
            .px_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().elevated_surface_background)
            .gap_1()
            .child(
                Button::new(
                    ElementId::Name("suggested-edits".into()),
                    RatePredictionView::SuggestedEdits.name(),
                )
                .label_size(LabelSize::Small)
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.current_view = RatePredictionView::SuggestedEdits;
                    cx.notify();
                }))
                .toggle_state(self.current_view == RatePredictionView::SuggestedEdits),
            )
            .child(
                Button::new(
                    ElementId::Name("raw-input".into()),
                    RatePredictionView::RawInput.name(),
                )
                .label_size(LabelSize::Small)
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.current_view = RatePredictionView::RawInput;
                    cx.notify();
                }))
                .toggle_state(self.current_view == RatePredictionView::RawInput),
            )
    }

    fn render_suggested_edits(&self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        let bg_color = cx.theme().colors().editor_background;
        let border_color = cx.theme().colors().border;
        let active_prediction = self.active_prediction.as_ref()?;

        Some(
            v_flex()
                .id("diff")
                .size_full()
                .bg(bg_color)
                .overflow_hidden()
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .child(
                            h_flex()
                                .h_8()
                                .px_2()
                                .border_b_1()
                                .border_color(border_color)
                                .child(Label::new("Predicted Patch").size(LabelSize::Small)),
                        )
                        .child(
                            div()
                                .id("predicted-patch-diff")
                                .p_4()
                                .flex_1()
                                .min_h_0()
                                .overflow_scroll()
                                .whitespace_nowrap()
                                .child(self.diff_editor.clone()),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .border_t_1()
                        .border_color(border_color)
                        .child(
                            h_flex()
                                .h_8()
                                .px_2()
                                .gap_2()
                                .border_b_1()
                                .border_color(border_color)
                                .child(Label::new("Expected Patch").size(LabelSize::Small)),
                        )
                        .child(
                            div()
                                .id("expected-patch")
                                .p_4()
                                .flex_1()
                                .min_h_0()
                                .overflow_scroll()
                                .whitespace_nowrap()
                                .child(active_prediction.expected_editor.clone()),
                        ),
                ),
        )
    }

    fn render_raw_input(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        let theme_settings = ThemeSettings::get_global(cx);
        let buffer_font_size = theme_settings.buffer_font_size(cx);

        Some(
            v_flex()
                .size_full()
                .overflow_hidden()
                .relative()
                .child(
                    div()
                        .id("raw-input")
                        .py_4()
                        .px_6()
                        .size_full()
                        .bg(cx.theme().colors().editor_background)
                        .overflow_scroll()
                        .child(if let Some(active_prediction) = &self.active_prediction {
                            markdown::MarkdownElement::new(
                                active_prediction.formatted_inputs.clone(),
                                MarkdownStyle {
                                    base_text_style: window.text_style(),
                                    syntax: cx.theme().syntax().clone(),
                                    code_block: StyleRefinement {
                                        text: TextStyleRefinement {
                                            font_family: Some(
                                                theme_settings.buffer_font.family.clone(),
                                            ),
                                            font_size: Some(buffer_font_size.into()),
                                            ..Default::default()
                                        },
                                        padding: EdgesRefinement {
                                            top: Some(DefiniteLength::Absolute(
                                                AbsoluteLength::Pixels(px(8.)),
                                            )),
                                            left: Some(DefiniteLength::Absolute(
                                                AbsoluteLength::Pixels(px(8.)),
                                            )),
                                            right: Some(DefiniteLength::Absolute(
                                                AbsoluteLength::Pixels(px(8.)),
                                            )),
                                            bottom: Some(DefiniteLength::Absolute(
                                                AbsoluteLength::Pixels(px(8.)),
                                            )),
                                        },
                                        margin: EdgesRefinement {
                                            top: Some(Length::Definite(px(8.).into())),
                                            left: Some(Length::Definite(px(0.).into())),
                                            right: Some(Length::Definite(px(0.).into())),
                                            bottom: Some(Length::Definite(px(12.).into())),
                                        },
                                        border_style: Some(BorderStyle::Solid),
                                        border_widths: EdgesRefinement {
                                            top: Some(AbsoluteLength::Pixels(px(1.))),
                                            left: Some(AbsoluteLength::Pixels(px(1.))),
                                            right: Some(AbsoluteLength::Pixels(px(1.))),
                                            bottom: Some(AbsoluteLength::Pixels(px(1.))),
                                        },
                                        border_color: Some(cx.theme().colors().border_variant),
                                        background: Some(
                                            cx.theme().colors().editor_background.into(),
                                        ),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                            )
                            .into_any_element()
                        } else {
                            div()
                                .child("No active completion".to_string())
                                .into_any_element()
                        }),
                )
                .id("raw-input-view"),
        )
    }

    fn render_active_completion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let active_prediction = self.active_prediction.as_ref()?;
        let completion_id = active_prediction.prediction.id.clone();
        let focus_handle = &self.focus_handle(cx);

        let border_color = cx.theme().colors().border;
        let bg_color = cx.theme().colors().editor_background;

        let rated = self.ep_store.read(cx).is_prediction_rated(&completion_id);
        let feedback_empty = active_prediction
            .feedback_editor
            .read(cx)
            .text(cx)
            .is_empty();

        let label_container = h_flex().pl_1().gap_1p5();

        Some(
            v_flex()
                .size_full()
                .overflow_hidden()
                .relative()
                .child(
                    v_flex()
                        .size_full()
                        .overflow_hidden()
                        .relative()
                        .child(self.render_view_nav(cx))
                        .when_some(
                            match self.current_view {
                                RatePredictionView::SuggestedEdits => {
                                    self.render_suggested_edits(cx)
                                }
                                RatePredictionView::RawInput => self.render_raw_input(window, cx),
                            },
                            |this, element| this.child(element),
                        ),
                )
                .when(!rated, |this| {
                    let modal = cx.entity().downgrade();
                    let failure_mode_menu =
                        ContextMenu::build(window, cx, move |menu, _window, _cx| {
                            FeedbackCompletionProvider::FAILURE_MODES
                                .iter()
                                .fold(menu, |menu, (key, description)| {
                                    let key: SharedString = (*key).into();
                                    let description: SharedString = (*description).into();
                                    let modal = modal.clone();
                                    menu.entry(
                                        format!("{} {}", key, description),
                                        None,
                                        move |window, cx| {
                                            if let Some(modal) = modal.upgrade() {
                                                modal.update(cx, |this, cx| {
                                                    if let Some(active) = &this.active_prediction {
                                                        active.feedback_editor.update(
                                                            cx,
                                                            |editor, cx| {
                                                                editor.set_text(
                                                                    format!("{} {}", key, description),
                                                                    window,
                                                                    cx,
                                                                );
                                                            },
                                                        );
                                                    }
                                                });
                                            }
                                        },
                                    )
                                })
                        });

                    this.child(
                        h_flex()
                            .p_2()
                            .gap_2()
                            .border_y_1()
                            .border_color(border_color)
                            .child(
                                DropdownMenu::new(
                                        "failure-mode-dropdown",
                                        "Issue",
                                        failure_mode_menu,
                                    )
                                    .handle(self.failure_mode_menu_handle.clone())
                                    .style(ui::DropdownStyle::Outlined)
                                    .trigger_size(ButtonSize::Compact),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Icon::new(IconName::Info)
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        div().flex_wrap().child(
                                            Label::new(concat!(
                                                "Explain why this completion is good or bad. ",
                                                "If it's negative, describe what you expected instead."
                                            ))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                        ),
                                    ),
                            ),
                    )
                })
                .when(!rated, |this| {
                    this.child(
                        div()
                            .h_40()
                            .pt_1()
                            .bg(bg_color)
                            .child(active_prediction.feedback_editor.clone()),
                    )
                })
                .child(
                    h_flex()
                        .p_1()
                        .h_8()
                        .max_h_8()
                        .border_t_1()
                        .border_color(border_color)
                        .max_w_full()
                        .justify_between()
                        .children(if rated {
                            Some(
                                label_container
                                    .child(
                                        Icon::new(IconName::Check)
                                            .size(IconSize::Small)
                                            .color(Color::Success),
                                    )
                                    .child(Label::new("Rated completion.").color(Color::Muted)),
                            )
                        } else if active_prediction.prediction.edits.is_empty() {
                            Some(
                                label_container
                                    .child(
                                        Icon::new(IconName::Warning)
                                            .size(IconSize::Small)
                                            .color(Color::Warning),
                                    )
                                    .child(Label::new("No edits produced.").color(Color::Muted)),
                            )
                        } else {
                            Some(label_container)
                        })
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Button::new("bad", "Bad Prediction")
                                        .start_icon(Icon::new(IconName::ThumbsDown).size(IconSize::Small))
                                        .disabled(rated || feedback_empty)
                                        .when(feedback_empty, |this| {
                                            this.tooltip(Tooltip::text(
                                                "Explain what's bad about it before reporting it",
                                            ))
                                        })
                                        .key_binding(KeyBinding::for_action_in(
                                            &ThumbsDownActivePrediction,
                                            focus_handle,
                                            cx,
                                        ))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if this.active_prediction.is_some() {
                                                this.thumbs_down_active(
                                                    &ThumbsDownActivePrediction,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        })),
                                )
                                .child(
                                    Button::new("good", "Good Prediction")
                                        .start_icon(Icon::new(IconName::ThumbsUp).size(IconSize::Small))
                                        .disabled(rated)
                                        .key_binding(KeyBinding::for_action_in(
                                            &ThumbsUpActivePrediction,
                                            focus_handle,
                                            cx,
                                        ))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if this.active_prediction.is_some() {
                                                this.thumbs_up_active(
                                                    &ThumbsUpActivePrediction,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        })),
                                ),
                        ),
                ),
        )
    }
}

impl Render for RatePredictionsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border_color = cx.theme().colors().border;

        h_flex()
            .key_context("RatePredictionModal")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_prev_edit))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_next_edit))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::thumbs_up_active))
            .on_action(cx.listener(Self::thumbs_down_active))
            .on_action(cx.listener(Self::focus_completions))
            .on_action(cx.listener(Self::preview_completion))
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(border_color)
            .w(window.viewport_size().width - px(320.))
            .h(window.viewport_size().height - px(300.))
            .rounded_lg()
            .shadow_lg()
            .child(
                v_flex()
                    .w_72()
                    .h_full()
                    .border_r_1()
                    .border_color(border_color)
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child({
                        let icons = self.ep_store.read(cx).icons(cx);
                        h_flex()
                            .h_8()
                            .px_2()
                            .justify_between()
                            .border_b_1()
                            .border_color(border_color)
                            .child(Icon::new(icons.base).size(IconSize::Small))
                            .child(
                                Label::new("From most recent to oldest")
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                    })
                    .child(
                        div()
                            .id("completion_list")
                            .p_0p5()
                            .h_full()
                            .overflow_y_scroll()
                            .child(
                                List::new()
                                    .empty_message(
                                        div()
                                            .p_2()
                                            .child(
                                                Label::new(concat!(
                                                    "No completions yet. ",
                                                    "Use the editor to generate some, ",
                                                    "and make sure to rate them!"
                                                ))
                                                .color(Color::Muted),
                                            )
                                            .into_any_element(),
                                    )
                                    .children(self.render_shown_completions(cx)),
                            ),
                    ),
            )
            .children(self.render_active_completion(window, cx))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if !this.failure_mode_menu_handle.is_deployed() {
                    cx.emit(DismissEvent);
                }
            }))
    }
}
