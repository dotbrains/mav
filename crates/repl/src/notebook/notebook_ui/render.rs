use super::*;

impl Render for NotebookEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("NotebookEditor");
        key_context.set(
            "notebook_mode",
            match self.notebook_mode {
                NotebookMode::Command => "command",
                NotebookMode::Edit => "edit",
            },
        );

        v_flex()
            .size_full()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenNotebook, window, cx| {
                this.open_notebook(&OpenNotebook, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ClearOutputs, window, cx| this.clear_outputs(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Run, window, cx| this.run_current_cell(&Run, window, cx)),
            )
            .on_action(
                cx.listener(|this, action, window, cx| this.run_and_advance(action, window, cx)),
            )
            .on_action(cx.listener(|this, _: &RunAll, window, cx| this.run_cells(window, cx)))
            .on_action(
                cx.listener(|this, _: &MoveCellUp, window, cx| this.move_cell_up(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &MoveCellDown, window, cx| this.move_cell_down(window, cx)),
            )
            .on_action(cx.listener(|this, _: &AddMarkdownBlock, window, cx| {
                this.add_markdown_block(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &AddCodeBlock, window, cx| this.add_code_block(window, cx)),
            )
            .on_action(
                cx.listener(|this, action, window, cx| this.enter_edit_mode(action, window, cx)),
            )
            .on_action(cx.listener(|this, action, window, cx| {
                this.handle_enter_command_mode(action, window, cx)
            }))
            .on_action(cx.listener(|this, action, window, cx| {
                this.select_next(action, SelectionMode::SelectOnly, window, cx)
            }))
            .on_action(cx.listener(|this, action, window, cx| {
                this.select_previous(action, SelectionMode::SelectOnly, window, cx)
            }))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.select_next(
                    &Default::default(),
                    SelectionMode::SelectAndMove,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.select_previous(
                    &Default::default(),
                    SelectionMode::SelectAndMove,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &NotebookMoveDown, window, cx| {
                let Some(cell) = this.get_selected_cell() else {
                    return;
                };

                let Some(editor) = cell.editor(cx).cloned() else {
                    return;
                };

                let is_at_last_line = editor.update(cx, |editor, cx| {
                    let display_snapshot = editor.display_snapshot(cx);
                    let selections = editor.selections.all_display(&display_snapshot);
                    if let Some(selection) = selections.last() {
                        let head = selection.head();
                        let cursor_row = head.row();
                        let max_row = display_snapshot.max_point().row();

                        cursor_row >= max_row
                    } else {
                        false
                    }
                });

                if is_at_last_line {
                    this.select_next(
                        &Default::default(),
                        SelectionMode::SelectAndMove,
                        window,
                        cx,
                    );
                } else {
                    editor.update(cx, |editor, cx| {
                        editor.move_down(&Default::default(), window, cx);
                    });
                }
            }))
            .on_action(cx.listener(|this, _: &NotebookMoveUp, window, cx| {
                let Some(cell) = this.get_selected_cell() else {
                    return;
                };

                let Some(editor) = cell.editor(cx).cloned() else {
                    return;
                };

                let is_at_first_line = editor.update(cx, |editor, cx| {
                    let display_snapshot = editor.display_snapshot(cx);
                    let selections = editor.selections.all_display(&display_snapshot);
                    if let Some(selection) = selections.first() {
                        let head = selection.head();
                        let cursor_row = head.row();

                        cursor_row.0 == 0
                    } else {
                        false
                    }
                });

                if is_at_first_line {
                    this.select_previous(
                        &Default::default(),
                        SelectionMode::SelectAndMove,
                        window,
                        cx,
                    );
                } else {
                    editor.update(cx, |editor, cx| {
                        editor.move_up(&Default::default(), window, cx);
                    });
                }
            }))
            .on_action(
                cx.listener(|this, action, window, cx| this.restart_kernel(action, window, cx)),
            )
            .on_action(
                cx.listener(|this, action, window, cx| this.interrupt_kernel(action, window, cx)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .h_full()
                    .gap_2()
                    .child(div().flex_1().h_full().child(self.cell_list(window, cx)))
                    .child(self.render_notebook_controls(window, cx)),
            )
            .child(self.render_kernel_status_bar(window, cx))
    }
}
