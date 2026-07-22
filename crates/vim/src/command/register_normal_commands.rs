use super::*;

pub(super) fn register_normal_commands(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, action: &VimNorm, window, cx| {
        let keystrokes = action
            .command
            .chars()
            .map(|c| Keystroke::parse(&c.to_string()).unwrap())
            .collect();
        vim.switch_mode(Mode::Normal, true, window, cx);
        if let Some(override_rows) = &action.override_rows {
            vim.update_editor(cx, |_, editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.replace_cursors_with(|map| {
                        override_rows
                            .iter()
                            .map(|row| Point::new(*row, 0).to_display_point(map))
                            .collect()
                    });
                });
            });
        } else if let Some(range) = &action.range {
            let result = vim.update_editor(cx, |vim, editor, cx| {
                let range = range.buffer_range(vim, editor, window, cx)?;
                editor.change_selections(
                    SelectionEffects::no_scroll().nav_history(false),
                    window,
                    cx,
                    |s| {
                        s.select_ranges(
                            (range.start.0..=range.end.0)
                                .map(|line| Point::new(line, 0)..Point::new(line, 0)),
                        );
                    },
                );
                anyhow::Ok(())
            });
            if let Some(Err(err)) = result {
                log::error!("Error selecting range: {}", err);
                return;
            }
        };

        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };
        let task = workspace.update(cx, |workspace, cx| {
            workspace.send_keystrokes_impl(keystrokes, window, cx)
        });
        let had_range = action.range.is_some();
        let had_override = action.override_rows.is_some();

        cx.spawn_in(window, async move |vim, cx| {
            task.await;
            vim.update_in(cx, |vim, window, cx| {
                if matches!(vim.mode, Mode::Insert | Mode::Replace) {
                    vim.normal_before(&Default::default(), window, cx);
                } else {
                    vim.switch_mode(Mode::Normal, true, window, cx);
                }
                if had_override || had_range {
                    vim.update_editor(cx, |_, editor, cx| {
                        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                            s.select_anchor_ranges([s.newest_anchor().range()]);
                        });
                        if let Some(tx_id) = editor
                            .buffer()
                            .update(cx, |multi, cx| multi.last_transaction_id(cx))
                        {
                            let last_sel = editor.selections.disjoint_anchors_arc();
                            editor.modify_transaction_selection_history(tx_id, |old| {
                                old.0 = old.0.get(..1).unwrap_or(&[]).into();
                                old.1 = Some(last_sel);
                            });
                        }
                    });
                }
            })
            .log_err();
        })
        .detach();
    });

    Vim::action(editor, cx, |vim, _: &CountCommand, window, cx| {
        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };
        let count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        let n = if count > 1 {
            format!(".,.+{}", count.saturating_sub(1))
        } else {
            ".".to_string()
        };
        workspace.update(cx, |workspace, cx| {
            command_palette::CommandPalette::toggle(workspace, &n, window, cx);
        })
    });

    Vim::action(editor, cx, |vim, action: &GoToLine, window, cx| {
        vim.switch_mode(Mode::Normal, false, window, cx);
        let result = vim.update_editor(cx, |vim, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let buffer_row = action.range.head().buffer_row(vim, editor, window, cx)?;
            let current = editor
                .selections
                .newest::<Point>(&editor.display_snapshot(cx));
            let target = snapshot
                .buffer_snapshot()
                .clip_point(Point::new(buffer_row.0, current.head().column), Bias::Left);
            editor.change_selections(Default::default(), window, cx, |s| {
                s.select_ranges([target..target]);
            });

            anyhow::Ok(())
        });
        if let Some(e @ Err(_)) = result {
            let Some(workspace) = vim.workspace(window, cx) else {
                return;
            };
            workspace.update(cx, |workspace, cx| {
                e.notify_err(workspace, cx);
            });
        }
    });

    Vim::action(editor, cx, |vim, action: &YankCommand, window, cx| {
        vim.update_editor(cx, |vim, editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            if let Ok(range) = action.range.buffer_range(vim, editor, window, cx) {
                let end = if range.end < snapshot.buffer_snapshot().max_row() {
                    Point::new(range.end.0 + 1, 0)
                } else {
                    snapshot.buffer_snapshot().max_point()
                };
                vim.copy_ranges(
                    editor,
                    MotionKind::Linewise,
                    true,
                    vec![Point::new(range.start.0, 0)..end],
                    window,
                    cx,
                )
            }
        });
    });
}
