use super::*;

impl Console {
    pub(super) fn previous_query(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_query = self.query_bar.read(cx).text(cx);
        let prev = self.history.previous(&mut self.cursor, &current_query);
        if let Some(prev) = prev {
            self.query_bar.update(cx, |editor, cx| {
                editor.set_text(prev, window, cx);
            });
        }
    }

    pub(super) fn next_query(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.history.next(&mut self.cursor);
        let query = next.unwrap_or_else(|| {
            self.cursor.reset();
            ""
        });

        self.query_bar.update(cx, |editor, cx| {
            editor.set_text(query, window, cx);
        });
    }

    pub(super) fn evaluate(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let expression = self.query_bar.update(cx, |editor, cx| {
            let expression = editor.text(cx);
            cx.defer_in(window, |editor, window, cx| {
                editor.clear(window, cx);
            });

            expression
        });

        self.history.add(&mut self.cursor, expression.clone());
        self.cursor.reset();
        self.session.update(cx, |session, cx| {
            session
                .evaluate(
                    expression,
                    Some(dap::EvaluateArgumentsContext::Repl),
                    self.stack_frame_list.read(cx).opened_stack_frame_id(),
                    None,
                    cx,
                )
                .detach();
        });
    }

    pub(super) fn render_submit_menu(
        &self,
        id: impl Into<ElementId>,
        keybinding_target: Option<FocusHandle>,
        cx: &App,
    ) -> impl IntoElement {
        PopoverMenu::new(id.into())
            .trigger(
                ui::ButtonLike::new_rounded_right("console-confirm-split-button-right")
                    .layer(ui::ElevationIndex::ModalSurface)
                    .size(ui::ButtonSize::None)
                    .child(
                        div()
                            .px_1()
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    ),
            )
            .when(
                self.stack_frame_list
                    .read(cx)
                    .opened_stack_frame_id()
                    .is_some(),
                |this| {
                    this.menu(move |window, cx| {
                        Some(ContextMenu::build(window, cx, |context_menu, _, _| {
                            context_menu
                                .when_some(keybinding_target.clone(), |el, keybinding_target| {
                                    el.context(keybinding_target)
                                })
                                .action("Watch Expression", WatchExpression.boxed_clone())
                        }))
                    })
                },
            )
            .anchor(gpui::Anchor::TopRight)
    }

    pub(super) fn render_console(&self, cx: &Context<Self>) -> impl IntoElement {
        EditorElement::new(&self.console, Self::editor_style(&self.console, cx))
    }

    pub(super) fn editor_style(editor: &Entity<Editor>, cx: &Context<Self>) -> EditorStyle {
        let is_read_only = editor.read(cx).read_only(cx);
        let settings = ThemeSettings::get_global(cx);
        let theme = cx.theme();
        let text_style = TextStyle {
            color: if is_read_only {
                theme.colors().text_muted
            } else {
                theme.colors().text
            },
            font_family: settings.buffer_font.family.clone(),
            font_features: settings.buffer_font.features.clone(),
            font_size: settings.buffer_font_size(cx).into(),
            font_weight: settings.buffer_font.weight,
            line_height: relative(settings.buffer_line_height.value()),
            ..Default::default()
        };
        EditorStyle {
            background: theme.colors().editor_background,
            local_player: theme.players().local(),
            text: text_style,
            ..Default::default()
        }
    }

    pub(super) fn render_query_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        EditorElement::new(&self.query_bar, Self::editor_style(&self.query_bar, cx))
    }
}
