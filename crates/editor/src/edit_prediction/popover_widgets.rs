use super::*;

impl Editor {
    pub(crate) fn render_edit_prediction_inline_keystroke(
        &self,
        keystroke: &gpui::KeybindingKeystroke,
        modifiers_color: Color,
        cx: &App,
    ) -> AnyElement {
        let is_platform_style_mac = PlatformStyle::platform() == PlatformStyle::Mac;

        h_flex()
            .px_0p5()
            .when(is_platform_style_mac, |parent| parent.gap_0p5())
            .font(
                theme_settings::ThemeSettings::get_global(cx)
                    .buffer_font
                    .clone(),
            )
            .text_size(TextSize::XSmall.rems(cx))
            .child(h_flex().children(ui::render_modifiers(
                keystroke.modifiers(),
                PlatformStyle::platform(),
                Some(modifiers_color),
                Some(IconSize::XSmall.rems().into()),
                true,
            )))
            .when(is_platform_style_mac, |parent| {
                parent.child(keystroke.key().to_string())
            })
            .when(!is_platform_style_mac, |parent| {
                parent.child(
                    Key::new(ui::utils::capitalize(keystroke.key()), Some(Color::Default))
                        .size(Some(IconSize::XSmall.rems().into())),
                )
            })
            .into_any()
    }

    pub(crate) fn render_edit_prediction_popover_keystroke(
        &self,
        keystroke: &gpui::KeybindingKeystroke,
        color: Color,
        cx: &App,
    ) -> AnyElement {
        let is_platform_style_mac = PlatformStyle::platform() == PlatformStyle::Mac;

        if keystroke.modifiers().modified() {
            h_flex()
                .font(
                    theme_settings::ThemeSettings::get_global(cx)
                        .buffer_font
                        .clone(),
                )
                .when(is_platform_style_mac, |parent| parent.gap_1())
                .child(h_flex().children(ui::render_modifiers(
                    keystroke.modifiers(),
                    PlatformStyle::platform(),
                    Some(color),
                    None,
                    false,
                )))
                .into_any()
        } else {
            Key::new(ui::utils::capitalize(keystroke.key()), Some(color))
                .size(Some(IconSize::XSmall.rems().into()))
                .into_any_element()
        }
    }

    pub(crate) fn render_edit_prediction_keybind(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let keybind_display =
            self.edit_prediction_keybind_display(EditPredictionKeybindSurface::Inline, window, cx);
        let keystroke = keybind_display.displayed_keystroke.as_ref()?;

        let modifiers_color = if *keystroke.modifiers() == window.modifiers() {
            Color::Accent
        } else {
            Color::Muted
        };

        Some(self.render_edit_prediction_inline_keystroke(keystroke, modifiers_color, cx))
    }

    pub(crate) fn render_edit_prediction_line_popover(
        &self,
        label: impl Into<SharedString>,
        icon: Option<IconName>,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        let padding_right = if icon.is_some() { px(4.) } else { px(8.) };

        let keybind = self.render_edit_prediction_keybind(window, cx);
        let has_keybind = keybind.is_some();
        let icons = Self::get_prediction_provider_icons(&self.edit_prediction_provider, cx);

        h_flex()
            .id("ep-line-popover")
            .py_0p5()
            .pl_1()
            .pr(padding_right)
            .gap_1()
            .rounded_md()
            .border_1()
            .bg(Self::edit_prediction_line_popover_bg_color(cx))
            .border_color(Self::edit_prediction_callout_popover_border_color(cx))
            .shadow_xs()
            .when(!has_keybind, |el| {
                let status_colors = cx.theme().status();

                el.bg(status_colors.error_background)
                    .border_color(status_colors.error.opacity(0.6))
                    .pl_2()
                    .child(Icon::new(icons.error).color(Color::Error))
                    .cursor_default()
                    .hoverable_tooltip(move |_window, cx| {
                        cx.new(|_| MissingEditPredictionKeybindingTooltip).into()
                    })
            })
            .children(keybind)
            .child(
                Label::new(label)
                    .size(LabelSize::Small)
                    .when(!has_keybind, |el| {
                        el.color(cx.theme().status().error.into()).strikethrough()
                    }),
            )
            .when(!has_keybind, |el| {
                el.child(
                    h_flex().ml_1().child(
                        Icon::new(IconName::Info)
                            .size(IconSize::Small)
                            .color(cx.theme().status().error.into()),
                    ),
                )
            })
            .when_some(icon, |element, icon| {
                element.child(
                    div()
                        .mt(px(1.5))
                        .child(Icon::new(icon).size(IconSize::Small)),
                )
            })
    }

    pub(crate) fn render_edit_prediction_jump_outside_popover(
        &self,
        snapshot: &BufferSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        let keybind = self.render_edit_prediction_keybind(window, cx);
        let has_keybind = keybind.is_some();
        let icons = Self::get_prediction_provider_icons(&self.edit_prediction_provider, cx);

        let file_name = snapshot
            .file()
            .map(|file| SharedString::new(file.file_name(cx)))
            .unwrap_or(SharedString::new_static("untitled"));

        h_flex()
            .id("ep-jump-outside-popover")
            .py_1()
            .px_2()
            .gap_1()
            .rounded_md()
            .border_1()
            .bg(Self::edit_prediction_line_popover_bg_color(cx))
            .border_color(Self::edit_prediction_callout_popover_border_color(cx))
            .shadow_xs()
            .when(!has_keybind, |el| {
                let status_colors = cx.theme().status();

                el.bg(status_colors.error_background)
                    .border_color(status_colors.error.opacity(0.6))
                    .pl_2()
                    .child(Icon::new(icons.error).color(Color::Error))
                    .cursor_default()
                    .hoverable_tooltip(move |_window, cx| {
                        cx.new(|_| MissingEditPredictionKeybindingTooltip).into()
                    })
            })
            .children(keybind)
            .child(
                Label::new(file_name)
                    .size(LabelSize::Small)
                    .buffer_font(cx)
                    .when(!has_keybind, |el| {
                        el.color(cx.theme().status().error.into()).strikethrough()
                    }),
            )
            .when(!has_keybind, |el| {
                el.child(
                    h_flex().ml_1().child(
                        Icon::new(IconName::Info)
                            .size(IconSize::Small)
                            .color(cx.theme().status().error.into()),
                    ),
                )
            })
            .child(
                div()
                    .mt(px(1.5))
                    .child(Icon::new(IconName::ArrowUpRight).size(IconSize::Small)),
            )
    }

    pub(crate) fn edit_prediction_line_popover_bg_color(cx: &App) -> Hsla {
        let accent_color = cx.theme().colors().text_accent;
        let editor_bg_color = cx.theme().colors().editor_background;
        editor_bg_color.blend(accent_color.opacity(0.1))
    }

    pub(crate) fn edit_prediction_callout_popover_border_color(cx: &App) -> Hsla {
        let accent_color = cx.theme().colors().text_accent;
        let editor_bg_color = cx.theme().colors().editor_background;
        editor_bg_color.blend(accent_color.opacity(0.6))
    }

    pub(crate) fn get_prediction_provider_icons(
        provider: &Option<RegisteredEditPredictionDelegate>,
        cx: &App,
    ) -> edit_prediction_types::EditPredictionIconSet {
        match provider {
            Some(provider) => provider.provider.icons(cx),
            None => edit_prediction_types::EditPredictionIconSet::new(IconName::MavPredict),
        }
    }

    pub(crate) fn render_edit_prediction_cursor_popover_preview(
        &self,
        completion: &EditPredictionState,
        cursor_point: Point,
        style: &EditorStyle,
        cx: &mut Context<Editor>,
    ) -> Option<Div> {
        use text::ToPoint as _;

        pub(crate) fn render_relative_row_jump(
            prefix: impl Into<String>,
            current_row: u32,
            target_row: u32,
        ) -> Div {
            let (row_diff, arrow) = if target_row < current_row {
                (current_row - target_row, IconName::ArrowUp)
            } else {
                (target_row - current_row, IconName::ArrowDown)
            };

            h_flex()
                .child(
                    Label::new(format!("{}{}", prefix.into(), row_diff))
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                )
                .child(Icon::new(arrow).color(Color::Muted).size(IconSize::Small))
        }

        let supports_jump = self
            .edit_prediction_provider
            .as_ref()
            .map(|provider| provider.provider.supports_jump_to_edit())
            .unwrap_or(true);

        let icons = Self::get_prediction_provider_icons(&self.edit_prediction_provider, cx);

        match &completion.completion {
            EditPrediction::MoveWithin {
                target, snapshot, ..
            } => {
                if !supports_jump {
                    return None;
                }
                let (target, _) = self.display_snapshot(cx).anchor_to_buffer_anchor(*target)?;

                Some(
                    h_flex()
                        .px_2()
                        .gap_2()
                        .flex_1()
                        .child(if target.to_point(snapshot).row > cursor_point.row {
                            Icon::new(icons.down)
                        } else {
                            Icon::new(icons.up)
                        })
                        .child(Label::new("Jump to Edit")),
                )
            }
            EditPrediction::MoveOutside { snapshot, .. } => {
                let file_name = snapshot
                    .file()
                    .map(|file| file.file_name(cx))
                    .unwrap_or("untitled");
                Some(
                    h_flex()
                        .px_2()
                        .gap_2()
                        .flex_1()
                        .child(Icon::new(icons.base))
                        .child(Label::new(format!("Jump to {file_name}"))),
                )
            }
            EditPrediction::Edit {
                edits,
                edit_preview,
                snapshot,
                ..
            } => {
                let first_edit_row = self
                    .display_snapshot(cx)
                    .anchor_to_buffer_anchor(edits.first()?.0.start)?
                    .0
                    .to_point(snapshot)
                    .row;

                let (highlighted_edits, has_more_lines) =
                    if let Some(edit_preview) = edit_preview.as_ref() {
                        edit_prediction_edit_text(
                            snapshot,
                            edits,
                            edit_preview,
                            true,
                            &self.display_snapshot(cx),
                            cx,
                        )
                        .first_line_preview()
                    } else {
                        edit_prediction_fallback_text(edits, cx).first_line_preview()
                    };

                let styled_text = gpui::StyledText::new(highlighted_edits.text)
                    .with_default_highlights(&style.text, highlighted_edits.highlights);

                let preview = h_flex()
                    .gap_1()
                    .min_w_16()
                    .child(styled_text)
                    .when(has_more_lines, |parent| parent.child("…"));

                let left = if supports_jump && first_edit_row != cursor_point.row {
                    render_relative_row_jump("", cursor_point.row, first_edit_row)
                        .into_any_element()
                } else {
                    Icon::new(icons.base).into_any_element()
                };

                Some(
                    h_flex()
                        .h_full()
                        .flex_1()
                        .gap_2()
                        .pr_1()
                        .overflow_x_hidden()
                        .font(
                            theme_settings::ThemeSettings::get_global(cx)
                                .buffer_font
                                .clone(),
                        )
                        .child(left)
                        .child(preview),
                )
            }
        }
    }
}
