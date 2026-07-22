use super::*;

impl OnMatchingLines {
    // convert a vim query into something more usable by mav.
    // we don't attempt to fully convert between the two regex syntaxes,
    // but we do flip \( and \) to ( and ) (and vice-versa) in the pattern,
    // and convert \0..\9 to $0..$9 in the replacement so that common idioms work.
    pub(crate) fn parse(
        query: &str,
        range: &Option<CommandRange>,
    ) -> Option<(String, CommandRange, String, bool)> {
        let mut global = "global".chars().peekable();
        let mut query_chars = query.chars().peekable();
        let mut invert = false;
        if query_chars.peek() == Some(&'v') {
            invert = true;
            query_chars.next();
        }
        while global
            .peek()
            .is_some_and(|char| Some(char) == query_chars.peek())
        {
            global.next();
            query_chars.next();
        }
        if !invert && query_chars.peek() == Some(&'!') {
            invert = true;
            query_chars.next();
        }
        let range = range.clone().unwrap_or(CommandRange {
            start: Position::Line { row: 0, offset: 0 },
            end: Some(Position::LastLine { offset: 0 }),
        });

        let delimiter = query_chars.next().filter(|c| {
            !c.is_alphanumeric() && *c != '"' && *c != '|' && *c != '\'' && *c != '!'
        })?;

        let mut search = String::new();
        let mut escaped = false;

        for c in query_chars.by_ref() {
            if escaped {
                escaped = false;
                // unescape escaped parens
                if c != '(' && c != ')' && c != delimiter {
                    search.push('\\')
                }
                search.push(c)
            } else if c == '\\' {
                escaped = true;
            } else if c == delimiter {
                break;
            } else {
                // escape unescaped parens
                if c == '(' || c == ')' {
                    search.push('\\')
                }
                search.push(c)
            }
        }

        Some((query_chars.collect::<String>(), range, search, invert))
    }

    pub fn run(&self, vim: &mut Vim, window: &mut Window, cx: &mut Context<Vim>) {
        let result = vim.update_editor(cx, |vim, editor, cx| {
            self.range.buffer_range(vim, editor, window, cx)
        });

        let range = match result {
            None => return,
            Some(e @ Err(_)) => {
                let Some(workspace) = vim.workspace(window, cx) else {
                    return;
                };
                workspace.update(cx, |workspace, cx| {
                    e.notify_err(workspace, cx);
                });
                return;
            }
            Some(Ok(result)) => result,
        };

        let mut action = self.action.boxed_clone();
        let mut last_pattern = self.search.clone();

        let mut regexes = match Regex::new(&self.search) {
            Ok(regex) => vec![(regex, !self.invert)],
            e @ Err(_) => {
                let Some(workspace) = vim.workspace(window, cx) else {
                    return;
                };
                workspace.update(cx, |workspace, cx| {
                    e.notify_err(workspace, cx);
                });
                return;
            }
        };
        while let Some(inner) = action
            .boxed_clone()
            .as_any()
            .downcast_ref::<OnMatchingLines>()
        {
            let Some(regex) = Regex::new(&inner.search).ok() else {
                break;
            };
            last_pattern = inner.search.clone();
            action = inner.action.boxed_clone();
            regexes.push((regex, !inner.invert))
        }

        if let Some(pane) = vim.pane(window, cx) {
            pane.update(cx, |pane, cx| {
                if let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>()
                {
                    search_bar.update(cx, |search_bar, cx| {
                        if search_bar.show(window, cx) {
                            let _ = search_bar.search(
                                &last_pattern,
                                Some(SearchOptions::REGEX | SearchOptions::CASE_SENSITIVE),
                                false,
                                window,
                                cx,
                            );
                        }
                    });
                }
            });
        };

        vim.update_editor(cx, |_, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let mut row = range.start.0;

            let point_range = Point::new(range.start.0, 0)
                ..snapshot
                    .buffer_snapshot()
                    .clip_point(Point::new(range.end.0 + 1, 0), Bias::Left);
            cx.spawn_in(window, async move |editor, cx| {
                let new_selections = cx
                    .background_spawn(async move {
                        let mut line = String::new();
                        let mut new_selections = Vec::new();
                        let chunks = snapshot
                            .buffer_snapshot()
                            .text_for_range(point_range)
                            .chain(["\n"]);

                        for chunk in chunks {
                            for (newline_ix, text) in chunk.split('\n').enumerate() {
                                if newline_ix > 0 {
                                    if regexes.iter().all(|(regex, should_match)| {
                                        regex.is_match(&line) == *should_match
                                    }) {
                                        new_selections
                                            .push(Point::new(row, 0).to_display_point(&snapshot))
                                    }
                                    row += 1;
                                    line.clear();
                                }
                                line.push_str(text)
                            }
                        }

                        new_selections
                    })
                    .await;

                if new_selections.is_empty() {
                    return;
                }

                if let Some(vim_norm) = action.as_any().downcast_ref::<VimNorm>() {
                    let mut vim_norm = vim_norm.clone();
                    vim_norm.override_rows =
                        Some(new_selections.iter().map(|point| point.row().0).collect());
                    editor
                        .update_in(cx, |_, window, cx| {
                            window.dispatch_action(vim_norm.boxed_clone(), cx);
                        })
                        .log_err();
                    return;
                }

                editor
                    .update_in(cx, |editor, window, cx| {
                        editor.start_transaction_at(Instant::now(), window, cx);
                        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                            s.replace_cursors_with(|_| new_selections);
                        });
                        window.dispatch_action(action, cx);

                        cx.defer_in(window, move |editor, window, cx| {
                            let newest = editor
                                .selections
                                .newest::<Point>(&editor.display_snapshot(cx));
                            editor.change_selections(
                                SelectionEffects::no_scroll(),
                                window,
                                cx,
                                |s| {
                                    s.select(vec![newest]);
                                },
                            );
                            editor.end_transaction_at(Instant::now(), cx);
                        })
                    })
                    .log_err();
            })
            .detach();
        });
    }
}
