use super::*;

impl BufferSearchBar {
    pub(super) fn render_split_buttons(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let has_splittable_editor = self.splittable_editor.is_some();
        let split_buttons = if has_splittable_editor {
            self.splittable_editor
                .as_ref()
                .and_then(|weak| weak.upgrade())
                .map(|splittable_editor| {
                    let editor_ref = splittable_editor.read(cx);
                    let diff_view_style = editor_ref.diff_view_style();

                    let is_split_set = diff_view_style == DiffViewStyle::Split;
                    let is_split_active = editor_ref.is_split();
                    let min_columns =
                        EditorSettings::get_global(cx).minimum_split_diff_width as u32;

                    let split_icon = if is_split_set && !is_split_active {
                        IconName::DiffSplitAuto
                    } else {
                        IconName::DiffSplit
                    };

                    h_flex()
                        .gap_1()
                        .child(
                            IconButton::new("diff-unified", IconName::DiffUnified)
                                .icon_size(IconSize::Small)
                                .toggle_state(diff_view_style == DiffViewStyle::Unified)
                                .tooltip(Tooltip::text("Unified"))
                                .on_click({
                                    let splittable_editor = splittable_editor.downgrade();
                                    move |_, window, cx| {
                                        update_settings_file(
                                            <dyn Fs>::global(cx),
                                            cx,
                                            |settings, _| {
                                                settings.editor.diff_view_style =
                                                    Some(DiffViewStyle::Unified);
                                            },
                                        );
                                        if diff_view_style == DiffViewStyle::Split {
                                            splittable_editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_split(
                                                        &ToggleSplitDiff,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    }
                                }),
                        )
                        .child(
                            IconButton::new("diff-split", split_icon)
                                .toggle_state(diff_view_style == DiffViewStyle::Split)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::element(move |_, cx| {
                                    let message = if is_split_set && !is_split_active {
                                        format!("Split when wider than {} columns", min_columns)
                                            .into()
                                    } else {
                                        SharedString::from("Split")
                                    };

                                    v_flex()
                                        .child(message)
                                        .child(
                                            h_flex()
                                                .gap_0p5()
                                                .text_ui_sm(cx)
                                                .text_color(Color::Muted.color(cx))
                                                .children(render_modifiers(
                                                    &gpui::Modifiers::secondary_key(),
                                                    PlatformStyle::platform(),
                                                    None,
                                                    Some(TextSize::Small.rems(cx).into()),
                                                    false,
                                                ))
                                                .child("click to change min width"),
                                        )
                                        .into_any()
                                }))
                                .on_click({
                                    let splittable_editor = splittable_editor.downgrade();
                                    move |_, window, cx| {
                                        if window.modifiers().secondary() {
                                            window.dispatch_action(
                                                OpenSettingsAt {
                                                    path: "minimum_split_diff_width".to_string(),
                                                    target: None,
                                                }
                                                .boxed_clone(),
                                                cx,
                                            );
                                        } else {
                                            update_settings_file(
                                                <dyn Fs>::global(cx),
                                                cx,
                                                |settings, _| {
                                                    settings.editor.diff_view_style =
                                                        Some(DiffViewStyle::Split);
                                                },
                                            );
                                            if diff_view_style == DiffViewStyle::Unified {
                                                splittable_editor
                                                    .update(cx, |editor, cx| {
                                                        editor.toggle_split(
                                                            &ToggleSplitDiff,
                                                            window,
                                                            cx,
                                                        );
                                                    })
                                                    .ok();
                                            }
                                        }
                                    }
                                }),
                        )
                })
        } else {
            None
        };
        split_buttons.map(IntoElement::into_any_element)
    }
}
