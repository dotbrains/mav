use super::*;

impl Render for ThreadSearchBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.query_editor.focus_handle(cx);
        let theme = cx.theme().colors();

        let has_matches = !self.matches.is_empty();
        let query_empty = self.query_editor.read(cx).text(cx).is_empty();
        let in_error_state = self.query_error || (!query_empty && !has_matches);

        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("AcpThreadSearchBar");

        let counter_text = self.active_match_text(cx).unwrap_or_default();

        let bar_row = h_flex()
            .track_focus(&focus_handle)
            .key_context(key_context)
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::select_next_match))
            .on_action(cx.listener(Self::select_prev_match))
            .on_action(cx.listener(Self::toggle_case_sensitive))
            .on_action(cx.listener(Self::toggle_whole_word))
            .on_action(cx.listener(Self::toggle_regex))
            .on_action(cx.listener(Self::focus_search))
            .w_full()
            .gap_2()
            .child(
                h_flex()
                    .min_h_8()
                    .min_w_32()
                    .flex_1()
                    .px_1p5()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.editor_background)
                    .rounded_md()
                    .child(div().px_1().flex_1().child(render_query_input(
                        &self.query_editor,
                        in_error_state,
                        cx,
                    )))
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_1()
                            .child(SearchOption::CaseSensitive.as_button(
                                self.options,
                                SearchSource::Buffer,
                                focus_handle.clone(),
                            ))
                            .child(SearchOption::WholeWord.as_button(
                                self.options,
                                SearchSource::Buffer,
                                focus_handle.clone(),
                            ))
                            .child(SearchOption::Regex.as_button(
                                self.options,
                                SearchSource::Buffer,
                                focus_handle.clone(),
                            )),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .child(nav_button(
                        "thread-search-prev",
                        IconName::ChevronLeft,
                        !has_matches,
                        "Previous Match",
                        &SelectPreviousThreadMatch,
                        focus_handle.clone(),
                    ))
                    .child(nav_button(
                        "thread-search-next",
                        IconName::ChevronRight,
                        !has_matches,
                        "Next Match",
                        &SelectNextThreadMatch,
                        focus_handle.clone(),
                    ))
                    .child(
                        div().ml_1().min_w(rems(2.5)).child(
                            Label::new(counter_text)
                                .size(LabelSize::Small)
                                .when(!has_matches, |this| this.color(Color::Muted)),
                        ),
                    )
                    .child(nav_button(
                        "thread-search-dismiss",
                        IconName::Close,
                        false,
                        "Close Search",
                        &DismissThreadSearch,
                        focus_handle,
                    )),
            );

        let error_row = self
            .query_error_message
            .clone()
            .map(|msg| Label::new(msg).size(LabelSize::Small).color(Color::Error));

        v_flex()
            .w_full()
            .p_1p5()
            .bg(theme.panel_background)
            .border_b_1()
            .border_color(theme.border.opacity(0.6))
            .child(bar_row)
            .children(error_row)
    }
}

fn render_query_input(editor: &Entity<Editor>, has_error: bool, app: &App) -> impl IntoElement {
    let theme = app.theme().colors();
    let (color, use_syntax) = if has_error {
        (Color::Error.color(app), false)
    } else {
        (theme.text, true)
    };

    let settings = ThemeSettings::get_global(app);

    let text_style = TextStyle {
        color,
        font_family: settings.ui_font.family.clone(),
        font_features: settings.ui_font.features.clone(),
        font_fallbacks: settings.ui_font.fallbacks.clone(),
        font_size: rems(0.875).into(),
        font_weight: settings.ui_font.weight,
        line_height: relative(1.3),
        ..TextStyle::default()
    };
    let mut style = EditorStyle {
        background: theme.editor_background,
        local_player: app.theme().players().local(),
        text: text_style,
        ..EditorStyle::default()
    };
    if use_syntax {
        style.syntax = app.theme().syntax().clone();
    }
    EditorElement::new(editor, style)
}

fn nav_button(
    id: &'static str,
    icon: IconName,
    disabled: bool,
    tooltip: &'static str,
    action: &'static dyn Action,
    focus_handle: FocusHandle,
) -> IconButton {
    let action_for_dispatch = action;
    IconButton::new(id, icon)
        .shape(IconButtonShape::Square)
        .disabled(disabled)
        .on_click({
            let focus_handle = focus_handle.clone();
            move |_, window, cx| {
                if !focus_handle.is_focused(window) {
                    window.focus(&focus_handle, cx);
                }
                window.dispatch_action(action_for_dispatch.boxed_clone(), cx);
            }
        })
        .tooltip(move |_window, cx| Tooltip::for_action_in(tooltip, action, &focus_handle, cx))
}
