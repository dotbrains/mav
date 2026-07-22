use super::*;

impl Console {
    pub(super) fn add_messages(
        &mut self,
        events: Vec<OutputEvent>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        self.console.update(cx, |_, cx| {
            cx.spawn_in(window, async move |console, cx| {
                let mut len = console
                    .update(cx, |this, cx| this.buffer().read(cx).len(cx))?
                    .0;
                let (output, spans, background_spans) = cx
                    .background_spawn(async move {
                        let mut all_spans = Vec::new();
                        let mut all_background_spans = Vec::new();
                        let mut to_insert = String::new();
                        let mut scratch = String::new();

                        for event in &events {
                            scratch.clear();
                            let trimmed_output = event.output.trim_end();
                            scratch.push_str(trimmed_output);
                            scratch.push('\n');
                            let parsed_output = terminal::parse_ansi_text(scratch.as_bytes());
                            let output = parsed_output.text;
                            to_insert.extend(output.chars());
                            let mut spans = parsed_output.foreground_spans;
                            let mut background_spans = parsed_output.background_spans;

                            for (range, _) in spans.iter_mut() {
                                let start_offset = len + range.start;
                                *range = start_offset..len + range.end;
                            }

                            for (range, _) in background_spans.iter_mut() {
                                let start_offset = len + range.start;
                                *range = start_offset..len + range.end;
                            }

                            len += output.len();

                            all_spans.extend(spans);
                            all_background_spans.extend(background_spans);
                        }
                        (to_insert, all_spans, all_background_spans)
                    })
                    .await;
                console.update_in(cx, |console, window, cx| {
                    console.set_read_only(false);
                    console.move_to_end(&editor::actions::MoveToEnd, window, cx);
                    console.insert(&output, window, cx);
                    console.set_read_only(true);

                    let buffer = console.buffer().read(cx).snapshot(cx);

                    for (range, color) in spans {
                        let Some(color) = color else { continue };
                        let start_offset = range.start;
                        let range = buffer.anchor_after(MultiBufferOffset(range.start))
                            ..buffer.anchor_before(MultiBufferOffset(range.end));
                        let style = HighlightStyle {
                            color: Some(terminal_view::terminal_element::convert_color(
                                &color,
                                cx.theme(),
                            )),
                            ..Default::default()
                        };
                        console.highlight_text_key(
                            HighlightKey::ConsoleAnsiHighlight(start_offset),
                            vec![range],
                            style,
                            false,
                            cx,
                        );
                    }

                    for (range, color) in background_spans {
                        let Some(color) = color else { continue };
                        let start_offset = range.start;
                        let range = buffer.anchor_after(MultiBufferOffset(range.start))
                            ..buffer.anchor_before(MultiBufferOffset(range.end));
                        let color_fn = background_color_fetcher(color);
                        console.highlight_background(
                            HighlightKey::ConsoleAnsiHighlight(start_offset),
                            &[range],
                            move |_, theme| color_fn(theme),
                            cx,
                        );
                    }

                    cx.notify();
                })?;

                Ok(())
            })
        })
    }

    pub fn watch_expression(
        &mut self,
        _: &WatchExpression,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                    expression.clone(),
                    Some(dap::EvaluateArgumentsContext::Repl),
                    self.stack_frame_list.read(cx).opened_stack_frame_id(),
                    None,
                    cx,
                )
                .detach();

            if let Some(stack_frame_id) = self.stack_frame_list.read(cx).opened_stack_frame_id() {
                session
                    .add_watcher(expression.into(), stack_frame_id, cx)
                    .detach();
            }
        });
    }

    pub(crate) fn update_output(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update_output_task.is_some() {
            return;
        }
        let session = self.session.clone();
        let token = self.last_token;
        self.update_output_task = Some(cx.spawn_in(window, async move |this, cx| {
            let Some((last_processed_token, task)) = session
                .update_in(cx, |session, window, cx| {
                    let (output, last_processed_token) = session.output(token);

                    this.update(cx, |this, cx| {
                        if last_processed_token == this.last_token {
                            return None;
                        }
                        Some((
                            last_processed_token,
                            this.add_messages(output.cloned().collect(), window, cx),
                        ))
                    })
                    .ok()
                    .flatten()
                })
                .ok()
                .flatten()
            else {
                _ = this.update(cx, |this, _| {
                    this.update_output_task.take();
                });
                return;
            };
            _ = task.await.log_err();
            _ = this.update(cx, |this, _| {
                this.last_token = last_processed_token;
                this.update_output_task.take();
            });
        }));
    }
}

fn background_color_fetcher(color: terminal::Color) -> impl Fn(&Theme) -> Hsla {
    move |theme| {
        if terminal::is_default_background_color(color) {
            theme.colors().terminal_background
        } else {
            terminal_view::terminal_element::convert_color(&color, theme)
        }
    }
}
