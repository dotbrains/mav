use super::*;

pub(super) fn register_range_commands(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |_, action: &WithCount, window, cx| {
        for _ in 0..action.count {
            window.dispatch_action(action.action.boxed_clone(), cx)
        }
    });

    Vim::action(editor, cx, |vim, action: &WithRange, window, cx| {
        let result = vim.update_editor(cx, |vim, editor, cx| {
            action.range.buffer_range(vim, editor, window, cx)
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

        let previous_selections = vim
            .update_editor(cx, |_, editor, cx| {
                let selections = action.restore_selection.then(|| {
                    editor
                        .selections
                        .disjoint_anchor_ranges()
                        .collect::<Vec<_>>()
                });
                let snapshot = editor.buffer().read(cx).snapshot(cx);
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    let end = Point::new(range.end.0, snapshot.line_len(range.end));
                    s.select_ranges([end..Point::new(range.start.0, 0)]);
                });
                selections
            })
            .flatten();
        window.dispatch_action(action.action.boxed_clone(), cx);
        cx.defer_in(window, move |vim, window, cx| {
            vim.update_editor(cx, |_, editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    if let Some(previous_selections) = previous_selections {
                        s.select_ranges(previous_selections);
                    } else {
                        s.select_ranges([
                            Point::new(range.start.0, 0)..Point::new(range.start.0, 0)
                        ]);
                    }
                })
            });
        });
    });

    Vim::action(editor, cx, |vim, action: &OnMatchingLines, window, cx| {
        action.run(vim, window, cx)
    });

    Vim::action(editor, cx, |vim, action: &ShellExec, window, cx| {
        action.run(vim, window, cx)
    })
}
