use super::*;

impl SkillCreatorPage {
    fn render_url_import(&self) -> impl IntoElement {
        v_flex()
            .flex_shrink_0()
            .gap_2()
            .child(
                h_flex()
                    .gap_1()
                    .child(Label::new("Import from URL"))
                    .child(Label::new("(optional)").color(Color::Muted)),
            )
            .child(self.url_editor.clone())
            .child(match &self.url_import_status {
                UrlImportStatus::Idle => Label::new(
                    "Paste a GitHub .md URL to fetch it and fill out the form. \
                     For private files, Mav retries using GITHUB_TOKEN, if set.",
                )
                .size(LabelSize::Small)
                .color(Color::Muted)
                .into_any_element(),
                UrlImportStatus::Fetching => {
                    LoadingLabel::new("Fetching and parsing…").into_any_element()
                }
                UrlImportStatus::Error(error) => h_flex()
                    .gap_1()
                    .child(
                        Icon::new(IconName::XCircle)
                            .size(IconSize::Small)
                            .color(Color::Error),
                    )
                    .child(
                        Label::new(error.clone())
                            .size(LabelSize::Small)
                            .color(Color::Error),
                    )
                    .into_any_element(),
            })
    }

    fn render_form_fields(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("skill-creator-form-fields")
            .flex_grow_1()
            .flex_shrink_0()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(Label::new("Front-matter"))
                    .child(self.name_editor.clone())
                    .child(self.description_editor.clone()),
            )
            .child(self.render_optional_params(cx))
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .flex_grow_1()
                    .flex_shrink_0()
                    .gap_2()
                    .child(Label::new("Skill Content"))
                    .child(self.render_body_field(window, cx))
                    .when_some(self.body_error, |this, error| {
                        this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
                    }),
            )
    }

    fn render_optional_params(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggle_state: ToggleState = self.disable_model_invocation.into();

        SwitchField::new(
            "disable-model-invocation",
            Some("Disable model invocation"),
            Some(
                "Hide this skill from the model's catalog. It can still be invoked via slash command."
                    .into(),
            ),
            toggle_state,
            cx.listener(|this, _state: &ToggleState, _window, cx| {
                this.toggle_disable_model_invocation(cx);
            }),
        )
        .tab_index(DISABLE_MODEL_INVOCATION_TAB_INDEX)
        .into_any_element()
    }

    fn render_body_field(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);
        let theme = cx.theme().clone();

        let has_error = self.body_error.is_some();

        let focus_handle = self
            .body_editor
            .focus_handle(cx)
            .tab_index(BODY_FIELD_TAB_INDEX)
            .tab_stop(true);

        let border_color = if has_error {
            theme.status().error_border
        } else if focus_handle.contains_focused(window, cx) {
            theme.colors().border_focused
        } else {
            theme.colors().border
        };

        div()
            .w_full()
            .flex_1()
            .min_h(px(160.))
            .p_2p5()
            .rounded_md()
            .border_1()
            .border_color(border_color)
            .bg(theme.colors().editor_background)
            .track_focus(&focus_handle)
            .overflow_hidden()
            .child(EditorElement::new(
                &self.body_editor,
                EditorStyle {
                    local_player: theme.players().local(),
                    text: TextStyle {
                        color: theme.colors().text,
                        font_family: settings.buffer_font.family.clone(),
                        font_features: settings.buffer_font.features.clone(),
                        font_size: rems(0.875).into(),
                        font_weight: settings.buffer_font.weight,
                        line_height: relative(settings.buffer_line_height.value()),
                        ..Default::default()
                    },
                    syntax: theme.syntax().clone(),
                    inlay_hints_style: editor::make_inlay_hints_style(cx),
                    edit_prediction_styles: editor::make_suggestion_styles(cx),
                    ..EditorStyle::default()
                },
            ))
    }

    fn render_footer(&self, _window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let saving = self.saving;
        let main_action = if saving { "Saving…" } else { "Save Skill" };

        v_flex()
            .w_full()
            .py_2p5()
            .px_8()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant.opacity(0.4))
            .when(self.save_error.is_some(), |this| {
                this.gap_2().child(
                    Banner::new()
                        .severity(Severity::Error)
                        .children(self.save_error.clone().map(|err| Label::new(err))),
                )
            })
            .child(
                h_flex().w_full().gap_1().justify_end().child(
                    Button::new("save-skill", main_action)
                        .size(ButtonSize::Medium)
                        .style(ButtonStyle::Outlined)
                        .loading(saving)
                        .tab_index(SAVE_BUTTON_TAB_INDEX)
                        // Call `save_skill` directly instead of dispatching the
                        // `SaveSkill` action: action dispatch follows the focused
                        // element's path, so a dispatched action is silently
                        // dropped whenever focus is outside the creator (e.g.
                        // right after switching the settings file/scope).
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_skill(&SaveSkill, window, cx);
                        })),
                ),
            )
    }
}

impl Render for SkillCreatorPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("skill-creator")
            .key_context("SkillCreator")
            .track_focus(&self.focus_handle)
            .on_action(
                |action_sequence: &ActionSequence, window: &mut Window, cx: &mut App| {
                    for action in &action_sequence.0 {
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                },
            )
            .on_action(cx.listener(Self::save_skill))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::focus_next_field))
            .on_action(cx.listener(Self::focus_previous_field))
            .on_action(cx.listener(Self::on_menu_next))
            .on_action(cx.listener(Self::on_menu_prev))
            .size_full()
            .overflow_hidden()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .vertical_scrollbar_for(&self.scroll_handle, window, cx)
                    .child(
                        v_flex()
                            .id("skill-creator-form")
                            .tab_index(0)
                            .tab_group()
                            .tab_stop(false)
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .gap_4()
                            .px_8()
                            .py_4()
                            .child(self.render_url_import())
                            .child(Divider::horizontal().flex_shrink_0().flex_grow_1())
                            .child(self.render_form_fields(window, cx)),
                    ),
            )
            .child(self.render_footer(window, cx))
    }
}
