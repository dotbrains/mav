use super::*;

impl Vim {
    pub(crate) fn find_command(
        &mut self,
        action: &FindCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.pane(window, cx) else {
            return;
        };
        pane.update(cx, |pane, cx| {
            if let Some(search_bar) = pane.toolbar().read(cx).item_of_type::<BufferSearchBar>() {
                let search = search_bar.update(cx, |search_bar, cx| {
                    if !search_bar.show(window, cx) {
                        return None;
                    }
                    let mut query = action.query.clone();
                    if query.is_empty() {
                        query = search_bar.query(cx);
                    };

                    let mut options = SearchOptions::REGEX | SearchOptions::CASE_SENSITIVE;
                    if search_bar.should_use_smartcase_search(cx) {
                        options.set(
                            SearchOptions::CASE_SENSITIVE,
                            search_bar.is_contains_uppercase(&query),
                        );
                    }

                    Some(search_bar.search(&query, Some(options), true, window, cx))
                });
                let Some(search) = search else { return };
                let search_bar = search_bar.downgrade();
                let direction = if action.backwards {
                    Direction::Prev
                } else {
                    Direction::Next
                };
                cx.spawn_in(window, async move |_, cx| {
                    search.await?;
                    search_bar.update_in(cx, |search_bar, window, cx| {
                        search_bar.select_match(direction, 1, window, cx)
                    })?;
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
        })
    }

    pub(crate) fn replace_command(
        &mut self,
        action: &ReplaceCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replacement = action.replacement.clone();
        let Some(((pane, workspace), editor)) = self
            .pane(window, cx)
            .zip(self.workspace(window, cx))
            .zip(self.editor())
        else {
            return;
        };
        if let Some(result) = self.update_editor(cx, |vim, editor, cx| {
            let range = action.range.buffer_range(vim, editor, window, cx)?;
            let snapshot = editor.snapshot(window, cx);
            let snapshot = snapshot.buffer_snapshot();
            let end_point = Point::new(range.end.0, snapshot.line_len(range.end));
            let range = snapshot.anchor_before(Point::new(range.start.0, 0))
                ..snapshot.anchor_after(end_point);
            editor.set_search_within_ranges(&[range], cx);
            anyhow::Ok(())
        }) {
            workspace.update(cx, |workspace, cx| {
                result.notify_err(workspace, cx);
            })
        }
        let Some(search_bar) = pane.update(cx, |pane, cx| {
            pane.toolbar().read(cx).item_of_type::<BufferSearchBar>()
        }) else {
            return;
        };
        let mut options = SearchOptions::REGEX;
        let search = search_bar.update(cx, |search_bar, cx| {
            if !search_bar.show(window, cx) {
                return None;
            }

            let search = if replacement.search.is_empty() {
                search_bar.query(cx)
            } else {
                replacement.search
            };

            if let Some(case) = replacement.case_sensitive {
                options.set(SearchOptions::CASE_SENSITIVE, case)
            } else if search_bar.should_use_smartcase_search(cx) {
                options.set(
                    SearchOptions::CASE_SENSITIVE,
                    search_bar.is_contains_uppercase(&search),
                );
            } else {
                // Fallback: no explicit i/I flags and smartcase disabled;
                // use global editor.search.case_sensitive.
                options.set(
                    SearchOptions::CASE_SENSITIVE,
                    EditorSettings::get_global(cx).search.case_sensitive,
                )
            }

            // gdefault inverts the behavior of the 'g' flag.
            let replace_all = VimSettings::get_global(cx).gdefault != replacement.flag_g;
            if !replace_all {
                options.set(SearchOptions::ONE_MATCH_PER_LINE, true);
            }

            search_bar.set_replacement(Some(&replacement.replacement), cx);
            if replacement.flag_c {
                search_bar.focus_replace(window, cx);
            }
            Some(search_bar.search(&search, Some(options), true, window, cx))
        });
        if replacement.flag_n {
            self.move_cursor(
                Motion::StartOfLine {
                    display_lines: false,
                },
                None,
                window,
                cx,
            );
            return;
        }
        let Some(search) = search else { return };
        let search_bar = search_bar.downgrade();
        cx.spawn_in(window, async move |vim, cx| {
            search.await?;
            search_bar.update_in(cx, |search_bar, window, cx| {
                if replacement.flag_c {
                    search_bar.select_first_match(window, cx);
                    return;
                }
                search_bar.select_last_match(window, cx);
                search_bar.replace_all(&Default::default(), window, cx);
                editor.update(cx, |editor, cx| editor.clear_search_within_ranges(cx));
                let _ = search_bar.search(&search_bar.query(cx), None, false, window, cx);
                vim.update(cx, |vim, cx| {
                    vim.move_cursor(
                        Motion::StartOfLine {
                            display_lines: false,
                        },
                        None,
                        window,
                        cx,
                    )
                })
                .ok();

                // Disable the `ONE_MATCH_PER_LINE` search option when finished, as
                // this is not properly supported outside of vim mode, and
                // not disabling it makes the "Replace All Matches" button
                // actually replace only the first match on each line.
                options.set(SearchOptions::ONE_MATCH_PER_LINE, false);
                search_bar.set_search_options(options, cx);
            })
        })
        .detach_and_log_err(cx);
    }
}
