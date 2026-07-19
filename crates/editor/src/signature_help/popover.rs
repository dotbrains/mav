use super::*;

impl SignatureHelpPopover {
    pub fn render(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let Some(signature) = self.signatures.get(self.current_signature) else {
            return div().into_any_element();
        };

        let editor = cx.weak_entity();
        let main_content = div()
            .occlude()
            .p_2()
            .child(
                div()
                    .id("signature_help_container")
                    .overflow_y_scroll()
                    .max_w(max_size.width)
                    .max_h(max_size.height)
                    .track_scroll(&self.scroll_handle)
                    .child(
                        StyledText::new(signature.label.clone()).with_default_highlights(
                            &self.style,
                            signature.highlights.iter().cloned(),
                        ),
                    )
                    .when_some(
                        signature.parameter_documentation.clone(),
                        |this, param_doc| {
                            this.child(div().h_px().bg(cx.theme().colors().border_variant).my_1())
                                .child(
                                    MarkdownElement::new(
                                        param_doc,
                                        hover_markdown_style(window, cx),
                                    )
                                    .code_block_renderer(markdown::CodeBlockRenderer::Default {
                                        copy_button_visibility: CopyButtonVisibility::Hidden,
                                        wrap_button_visibility:
                                            markdown::WrapButtonVisibility::Hidden,
                                        border: false,
                                    })
                                    .on_url_click({
                                        let editor = editor.clone();
                                        move |link, window, cx| {
                                            open_markdown_url(
                                                editor
                                                    .read_with(cx, |editor, _| editor.workspace())
                                                    .ok()
                                                    .flatten(),
                                                link,
                                                window,
                                                cx,
                                            )
                                        }
                                    }),
                                )
                        },
                    )
                    .when_some(signature.documentation.clone(), |this, description| {
                        this.child(div().h_px().bg(cx.theme().colors().border_variant).my_1())
                            .child(
                                MarkdownElement::new(description, hover_markdown_style(window, cx))
                                    .code_block_renderer(markdown::CodeBlockRenderer::Default {
                                        copy_button_visibility: CopyButtonVisibility::Hidden,
                                        wrap_button_visibility:
                                            markdown::WrapButtonVisibility::Hidden,
                                        border: false,
                                    })
                                    .on_url_click(move |link, window, cx| {
                                        open_markdown_url(
                                            editor
                                                .read_with(cx, |editor, _| editor.workspace())
                                                .ok()
                                                .flatten(),
                                            link,
                                            window,
                                            cx,
                                        )
                                    }),
                            )
                    }),
            )
            .vertical_scrollbar_for(&self.scroll_handle, window, cx);

        let controls = if self.signatures.len() > 1 {
            let prev_button = IconButton::new("signature_help_prev", IconName::ChevronUp)
                .shape(IconButtonShape::Square)
                .style(ButtonStyle::Subtle)
                .icon_size(IconSize::Small)
                .tooltip(move |_window, cx| {
                    ui::Tooltip::for_action("Previous Signature", &crate::SignatureHelpPrevious, cx)
                })
                .on_click(cx.listener(|editor, _, window, cx| {
                    editor.signature_help_prev(&crate::SignatureHelpPrevious, window, cx);
                }));

            let next_button = IconButton::new("signature_help_next", IconName::ChevronDown)
                .shape(IconButtonShape::Square)
                .style(ButtonStyle::Subtle)
                .icon_size(IconSize::Small)
                .tooltip(move |_window, cx| {
                    ui::Tooltip::for_action("Next Signature", &crate::SignatureHelpNext, cx)
                })
                .on_click(cx.listener(|editor, _, window, cx| {
                    editor.signature_help_next(&crate::SignatureHelpNext, window, cx);
                }));

            let page = Label::new(format!(
                "{}/{}",
                self.current_signature + 1,
                self.signatures.len()
            ))
            .size(LabelSize::Small);

            Some(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .px_0p5()
                    .py_0p5()
                    .children([
                        prev_button.into_any_element(),
                        div().child(page).into_any_element(),
                        next_button.into_any_element(),
                    ])
                    .into_any_element(),
            )
        } else {
            None
        };
        div()
            .elevation_2(cx)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .flex()
            .flex_row()
            .when_some(controls, |this, controls| {
                this.children(vec![
                    div().flex().items_end().child(controls),
                    div().w_px().bg(cx.theme().colors().border_variant),
                ])
            })
            .child(main_content)
            .into_any_element()
    }
}
