use super::*;

impl Render for KeybindingEditorModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.add_action_arguments_input(window, cx);

        let theme = cx.theme().colors();
        let matching_bindings_count = self.get_matching_bindings_count(cx);
        let key_context = self.key_context_internal(window, cx);
        let showing_completions = key_context.contains("showing_completions");

        v_flex()
            .w(rems(34.))
            .elevation_3(cx)
            .key_context(key_context)
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .when(!showing_completions, |this| {
                this.on_action(cx.listener(Self::focus_next))
                    .on_action(cx.listener(Self::focus_prev))
            })
            .child(
                Modal::new("keybinding_editor_modal", None)
                    .header(
                        ModalHeader::new().child(
                            v_flex()
                                .w_full()
                                .pb_1p5()
                                .mb_1()
                                .gap_0p5()
                                .border_b_1()
                                .border_color(theme.border_variant)
                                .when(!self.creating, |this| {
                                    this.child(Label::new(
                                        self.editing_keybind.action().humanized_name.clone(),
                                    ))
                                    .when_some(
                                        self.editing_keybind.action().documentation,
                                        |this, docs| {
                                            this.child(
                                                Label::new(docs)
                                                    .size(LabelSize::Small)
                                                    .color(Color::Muted),
                                            )
                                        },
                                    )
                                })
                                .when(self.creating, |this| {
                                    this.child(Label::new("Create Keybinding"))
                                }),
                        ),
                    )
                    .section(
                        Section::new().child(
                            v_flex()
                                .gap_2p5()
                                .when_some(
                                    self.creating
                                        .then_some(())
                                        .and_then(|_| self.action_editor.as_ref()),
                                    |this, selector| this.child(selector.clone()),
                                )
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(Label::new("Edit Keystroke"))
                                        .child(self.keybind_editor.clone())
                                        .child(h_flex().gap_px().when(
                                            matching_bindings_count > 0,
                                            |this| {
                                                let label = format!(
                                                    "There {} {} {} with the same keystrokes.",
                                                    if matching_bindings_count == 1 {
                                                        "is"
                                                    } else {
                                                        "are"
                                                    },
                                                    matching_bindings_count,
                                                    if matching_bindings_count == 1 {
                                                        "binding"
                                                    } else {
                                                        "bindings"
                                                    }
                                                );

                                                this.child(
                                                    Label::new(label)
                                                        .size(LabelSize::Small)
                                                        .color(Color::Muted),
                                                )
                                                .child(
                                                    Button::new("show_matching", "View")
                                                        .label_size(LabelSize::Small)
                                                        .end_icon(
                                                            Icon::new(IconName::ArrowUpRight)
                                                                .size(IconSize::Small)
                                                                .color(Color::Muted),
                                                        )
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.show_matching_bindings(
                                                                    window, cx,
                                                                );
                                                            },
                                                        )),
                                                )
                                            },
                                        )),
                                )
                                .when_some(self.action_arguments_editor.clone(), |this, editor| {
                                    this.child(
                                        v_flex()
                                            .gap_1()
                                            .child(Label::new("Edit Arguments"))
                                            .child(editor),
                                    )
                                })
                                .child(self.context_editor.clone())
                                .when_some(self.error.as_ref(), |this, error| {
                                    this.child(
                                        Banner::new()
                                            .severity(error.severity)
                                            .child(Label::new(error.content.clone())),
                                    )
                                }),
                        ),
                    )
                    .footer(
                        ModalFooter::new().end_slot(
                            h_flex()
                                .gap_1()
                                .child(
                                    Button::new("cancel", "Cancel")
                                        .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                                )
                                .child(Button::new("save-btn", "Save").on_click(cx.listener(
                                    |this, _event, _window, cx| {
                                        this.save_or_display_error(cx);
                                    },
                                ))),
                        ),
                    ),
            )
    }
}
