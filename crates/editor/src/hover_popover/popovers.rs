use super::*;

pub struct InfoPopover {
    pub symbol_range: RangeInEditor,
    pub parsed_content: Option<Entity<Markdown>>,
    pub scroll_handle: ScrollHandle,
    pub keyboard_grace: Rc<RefCell<bool>>,
    pub anchor: Option<Anchor>,
    pub last_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) _subscription: Option<Subscription>,
}

impl InfoPopover {
    pub(crate) fn render(
        &mut self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let keyboard_grace = Rc::clone(&self.keyboard_grace);
        let this = cx.entity().downgrade();
        let this2 = this.clone();
        let bounds_cell = self.last_bounds.clone();
        div()
            .id("info_popover")
            .occlude()
            .elevation_2(cx)
            .child(
                canvas(
                    {
                        move |bounds, _window, _cx| {
                            bounds_cell.set(Some(bounds));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            // Prevent a mouse down/move on the popover from being propagated to the editor,
            // because that would dismiss the popover.
            .on_mouse_move({
                move |_, _, cx: &mut App| {
                    this.update(cx, |editor, _| {
                        editor.hover_state.closest_mouse_distance = Some(px(0.0));
                        editor.hover_state.hiding_delay_task = None;
                    })
                    .ok();
                    cx.stop_propagation()
                }
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                let mut keyboard_grace = keyboard_grace.borrow_mut();
                *keyboard_grace = false;
                cx.stop_propagation();
            })
            .when_some(self.parsed_content.clone(), |this, markdown| {
                this.child(
                    div()
                        .id("info-md-container")
                        .overflow_y_scroll()
                        .max_w(max_size.width)
                        .max_h(max_size.height)
                        .track_scroll(&self.scroll_handle)
                        .child(
                            MarkdownElement::new(markdown, hover_markdown_style(window, cx))
                                .scroll_handle(self.scroll_handle.clone())
                                .code_block_renderer(::markdown::CodeBlockRenderer::Default {
                                    copy_button_visibility: CopyButtonVisibility::Hidden,
                                    wrap_button_visibility:
                                        ::markdown::WrapButtonVisibility::Hidden,
                                    border: false,
                                })
                                .on_url_click(move |link, window, cx| {
                                    open_markdown_url(
                                        this2
                                            .read_with(cx, |editor, _| editor.workspace())
                                            .ok()
                                            .flatten(),
                                        link,
                                        window,
                                        cx,
                                    )
                                })
                                .p_2(),
                        ),
                )
                .custom_scrollbars(
                    Scrollbars::for_settings::<EditorSettingsScrollbarProxy>()
                        .tracked_scroll_handle(&self.scroll_handle),
                    window,
                    cx,
                )
            })
            .into_any_element()
    }

    pub fn scroll(&self, amount: ScrollAmount, window: &mut Window, cx: &mut Context<Editor>) {
        let mut current = self.scroll_handle.offset();
        current.y -= amount.pixels(
            window.line_height(),
            self.scroll_handle.bounds().size.height - px(16.),
        ) / 2.0;
        cx.notify();
        self.scroll_handle.set_offset(current);
    }
}

pub struct DiagnosticPopover {
    pub(crate) local_diagnostic: DiagnosticEntry<Anchor>,
    pub(crate) markdown: Entity<Markdown>,
    pub(crate) border_color: Hsla,
    pub(crate) background_color: Hsla,
    pub keyboard_grace: Rc<RefCell<bool>>,
    pub anchor: Anchor,
    pub last_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub(crate) _subscription: Subscription,
    pub scroll_handle: ScrollHandle,
}

impl DiagnosticPopover {
    pub fn render(
        &self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let keyboard_grace = Rc::clone(&self.keyboard_grace);
        let this = cx.entity().downgrade();
        let bounds_cell = self.last_bounds.clone();
        div()
            .id("diagnostic")
            .occlude()
            .elevation_2_borderless(cx)
            .child(
                canvas(
                    {
                        move |bounds, _window, _cx| {
                            bounds_cell.set(Some(bounds));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            // Don't draw the background color if the theme
            // allows transparent surfaces.
            .when(theme_is_transparent(cx), |this| {
                this.bg(gpui::transparent_black())
            })
            // Prevent a mouse move on the popover from being propagated to the editor,
            // because that would dismiss the popover.
            .on_mouse_move({
                let this = this.clone();
                move |_, _, cx: &mut App| {
                    this.update(cx, |editor, _| {
                        editor.hover_state.closest_mouse_distance = Some(px(0.0));
                        editor.hover_state.hiding_delay_task = None;
                    })
                    .ok();
                    cx.stop_propagation()
                }
            })
            // Prevent a mouse down on the popover from being propagated to the editor,
            // because that would move the cursor.
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                let mut keyboard_grace = keyboard_grace.borrow_mut();
                *keyboard_grace = false;
                cx.stop_propagation();
            })
            .child(
                div()
                    .relative()
                    .py_1()
                    .pl_2()
                    .pr_8()
                    .bg(self.background_color)
                    .border_1()
                    .border_color(self.border_color)
                    .rounded_lg()
                    .child(
                        div()
                            .id("diagnostic-content-container")
                            .max_w(max_size.width)
                            .max_h(max_size.height)
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(
                                MarkdownElement::new(
                                    self.markdown.clone(),
                                    diagnostics_markdown_style(window, cx),
                                )
                                .code_block_renderer(::markdown::CodeBlockRenderer::Default {
                                    copy_button_visibility: CopyButtonVisibility::Hidden,
                                    wrap_button_visibility:
                                        ::markdown::WrapButtonVisibility::Hidden,
                                    border: false,
                                })
                                .on_url_click(
                                    move |link, window, cx| {
                                        if let Some(renderer) = GlobalDiagnosticRenderer::global(cx)
                                        {
                                            this.update(cx, |this, cx| {
                                                renderer.as_ref().open_link(this, link, window, cx);
                                            })
                                            .ok();
                                        }
                                    },
                                ),
                            ),
                    )
                    .child(div().absolute().top_1().right_1().child({
                        let message = self.local_diagnostic.diagnostic.message.clone();
                        CopyButton::new("copy-diagnostic", message).tooltip_label("Copy Diagnostic")
                    }))
                    .custom_scrollbars(
                        Scrollbars::for_settings::<EditorSettingsScrollbarProxy>()
                            .tracked_scroll_handle(&self.scroll_handle),
                        window,
                        cx,
                    ),
            )
            .into_any_element()
    }
}
