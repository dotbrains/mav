use super::*;

pub(crate) fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, Vim::insert_after);
    Vim::action(editor, cx, Vim::insert_before);
    Vim::action(editor, cx, Vim::insert_first_non_whitespace);
    Vim::action(editor, cx, Vim::insert_end_of_line);
    Vim::action(editor, cx, Vim::insert_line_above);
    Vim::action(editor, cx, Vim::insert_line_below);
    Vim::action(editor, cx, Vim::insert_empty_line_above);
    Vim::action(editor, cx, Vim::insert_empty_line_below);
    Vim::action(editor, cx, Vim::insert_at_previous);
    Vim::action(editor, cx, Vim::change_case);
    Vim::action(editor, cx, Vim::convert_to_upper_case);
    Vim::action(editor, cx, Vim::convert_to_lower_case);
    Vim::action(editor, cx, Vim::convert_to_rot13);
    Vim::action(editor, cx, Vim::convert_to_rot47);
    Vim::action(editor, cx, Vim::yank_line);
    Vim::action(editor, cx, Vim::yank_to_end_of_line);
    Vim::action(editor, cx, Vim::toggle_comments);
    Vim::action(editor, cx, Vim::toggle_block_comments);
    Vim::action(editor, cx, Vim::paste);
    Vim::action(editor, cx, Vim::show_location);

    Vim::action(editor, cx, |vim, _: &DeleteLeft, window, cx| {
        vim.record_current_action(cx);
        let times = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        vim.delete_motion(Motion::Left, times, forced_motion, window, cx);
    });
    Vim::action(editor, cx, |vim, _: &DeleteRight, window, cx| {
        vim.record_current_action(cx);
        let times = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        vim.delete_motion(Motion::Right, times, forced_motion, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &HelixDelete, window, cx| {
        vim.record_current_action(cx);
        let original_selections =
            vim.update_editor(cx, |_, editor, _| editor.selections.disjoint_anchors_arc());
        vim.update_editor(cx, |_, editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    if selection.is_empty() {
                        selection.end = movement::right(map, selection.end)
                    }
                })
            })
        });
        let transaction_id = vim.visual_delete(false, window, cx);
        if let (Some(original_selections), Some(transaction_id)) =
            (original_selections, transaction_id)
        {
            let updated = vim.update_editor(cx, |_, editor, _| {
                editor.modify_transaction_selection_history(transaction_id, |selections| {
                    selections.0 = original_selections;
                })
            });
            debug_assert_ne!(updated, Some(false));
        }
        vim.switch_mode(Mode::HelixNormal, true, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &HelixCollapseSelection, window, cx| {
        vim.update_editor(cx, |_, editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.move_with(&mut |map, selection| {
                    let mut point = selection.head();
                    if !selection.reversed && !selection.is_empty() {
                        point = movement::left(map, selection.head());
                    }
                    selection.collapse_to(point, selection.goal)
                });
            });
        });
    });

    Vim::action(editor, cx, |vim, _: &ChangeToEndOfLine, window, cx| {
        vim.start_recording(cx);
        let times = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        vim.change_motion(
            Motion::EndOfLine {
                display_lines: false,
            },
            times,
            forced_motion,
            window,
            cx,
        );
    });
    Vim::action(editor, cx, |vim, _: &DeleteToEndOfLine, window, cx| {
        vim.record_current_action(cx);
        let times = Vim::take_count(cx);
        let forced_motion = Vim::take_forced_motion(cx);
        vim.delete_motion(
            Motion::EndOfLine {
                display_lines: false,
            },
            times,
            forced_motion,
            window,
            cx,
        );
    });
    Vim::action(editor, cx, |vim, _: &JoinLines, window, cx| {
        vim.join_lines_impl(true, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &JoinLinesNoWhitespace, window, cx| {
        vim.join_lines_impl(false, window, cx);
    });

    Vim::action(editor, cx, |vim, _: &GoToPreviousReference, window, cx| {
        let count = Vim::take_count(cx);
        vim.update_editor(cx, |_, editor, cx| {
            let task = editor.go_to_reference_before_or_after_position(
                editor::Direction::Prev,
                count.unwrap_or(1),
                window,
                cx,
            );
            if let Some(task) = task {
                task.detach_and_log_err(cx);
            };
        });
    });

    Vim::action(editor, cx, |vim, _: &GoToNextReference, window, cx| {
        let count = Vim::take_count(cx);
        vim.update_editor(cx, |_, editor, cx| {
            let task = editor.go_to_reference_before_or_after_position(
                editor::Direction::Next,
                count.unwrap_or(1),
                window,
                cx,
            );
            if let Some(task) = task {
                task.detach_and_log_err(cx);
            };
        });
    });

    Vim::action(editor, cx, |vim, _: &Undo, window, cx| {
        let times = Vim::take_count(cx);
        Vim::take_forced_motion(cx);
        vim.update_editor(cx, |_, editor, cx| {
            for _ in 0..times.unwrap_or(1) {
                editor.undo(&editor::actions::Undo, window, cx);
            }
        });
    });
    Vim::action(editor, cx, |vim, _: &Redo, window, cx| {
        let times = Vim::take_count(cx);
        Vim::take_forced_motion(cx);
        vim.update_editor(cx, |_, editor, cx| {
            for _ in 0..times.unwrap_or(1) {
                editor.redo(&editor::actions::Redo, window, cx);
            }
        });
    });
    Vim::action(editor, cx, |vim, _: &UndoLastLine, window, cx| {
        Vim::take_forced_motion(cx);
        vim.update_editor(cx, |vim, editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let Some(last_change) = editor.change_list.last_before_grouping() else {
                return;
            };

            let anchors = last_change.to_vec();
            let mut last_row = None;
            let ranges: Vec<_> = anchors
                .iter()
                .filter_map(|anchor| {
                    let point = anchor.to_point(&snapshot);
                    if last_row == Some(point.row) {
                        return None;
                    }
                    last_row = Some(point.row);
                    let line_range = Point::new(point.row, 0)
                        ..Point::new(point.row, snapshot.line_len(MultiBufferRow(point.row)));
                    Some((
                        snapshot.anchor_before(line_range.start)
                            ..snapshot.anchor_after(line_range.end),
                        line_range,
                    ))
                })
                .collect();

            let edits = editor.buffer().update(cx, |buffer, cx| {
                let current_content = ranges
                    .iter()
                    .map(|(anchors, _)| {
                        buffer
                            .snapshot(cx)
                            .text_for_range(anchors.clone())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>();
                let mut content_before_undo = current_content.clone();
                let mut undo_count = 0;

                loop {
                    let undone_tx = buffer.undo(cx);
                    undo_count += 1;
                    let mut content_after_undo = Vec::new();

                    let mut line_changed = false;
                    for ((anchors, _), text_before_undo) in
                        ranges.iter().zip(content_before_undo.iter())
                    {
                        let snapshot = buffer.snapshot(cx);
                        let text_after_undo =
                            snapshot.text_for_range(anchors.clone()).collect::<String>();

                        if &text_after_undo != text_before_undo {
                            line_changed = true;
                        }
                        content_after_undo.push(text_after_undo);
                    }

                    content_before_undo = content_after_undo;
                    if !line_changed {
                        break;
                    }
                    if undone_tx == vim.undo_last_line_tx {
                        break;
                    }
                }

                let edits = ranges
                    .into_iter()
                    .zip(content_before_undo.into_iter().zip(current_content))
                    .filter_map(|((_, mut points), (mut old_text, new_text))| {
                        if new_text == old_text {
                            return None;
                        }
                        let common_suffix_starts_at = old_text
                            .char_indices()
                            .rev()
                            .zip(new_text.chars().rev())
                            .find_map(
                                |((i, a), b)| {
                                    if a != b { Some(i + a.len_utf8()) } else { None }
                                },
                            )
                            .unwrap_or(old_text.len());
                        points.end.column -= (old_text.len() - common_suffix_starts_at) as u32;
                        old_text = old_text.split_at(common_suffix_starts_at).0.to_string();
                        let common_prefix_len = old_text
                            .char_indices()
                            .zip(new_text.chars())
                            .find_map(|((i, a), b)| if a != b { Some(i) } else { None })
                            .unwrap_or(0);
                        points.start.column = common_prefix_len as u32;
                        old_text = old_text.split_at(common_prefix_len).1.to_string();

                        Some((points, old_text))
                    })
                    .collect::<Vec<_>>();

                for _ in 0..undo_count {
                    buffer.redo(cx);
                }
                edits
            });
            vim.undo_last_line_tx = editor.transact(window, cx, |editor, window, cx| {
                editor.change_list.invert_last_group();
                editor.edit(edits, cx);
                editor.change_selections(SelectionEffects::default(), window, cx, |s| {
                    s.select_anchor_ranges(anchors.into_iter().map(|a| a..a));
                })
            });
        });
    });

    repeat::register(editor, cx);
    scroll::register(editor, cx);
    search::register(editor, cx);
    substitute::register(editor, cx);
    increment::register(editor, cx);
}
