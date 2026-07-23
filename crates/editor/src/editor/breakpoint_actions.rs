use super::*;

impl Editor {
    pub(crate) fn add_edit_block(
        &mut self,
        anchor: Anchor,
        base_text: &str,
        placeholder_text: &str,
        confirm: Option<PromptEditorCallback>,
        cancel: Option<PromptEditorCallback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak_editor = cx.weak_entity();
        let bp_prompt = cx.new(|cx| {
            let mut prompt_editor =
                PromptEditor::new(weak_editor, placeholder_text, base_text, window, cx);

            if let Some(callback) = confirm {
                prompt_editor = prompt_editor.on_confirm(callback);
            }
            if let Some(callback) = cancel {
                prompt_editor = prompt_editor.on_cancel(callback);
            }

            prompt_editor
        });

        let height = bp_prompt.update(cx, |this, cx| {
            this.prompt
                .update(cx, |prompt, cx| prompt.max_point(cx).row().0 + 1 + 2)
        });
        let cloned_prompt = bp_prompt.clone();
        let blocks = vec![BlockProperties {
            style: BlockStyle::Sticky,
            placement: BlockPlacement::Above(anchor),
            height: Some(height),
            render: Arc::new(move |cx| {
                *cloned_prompt.read(cx).editor_margins.lock() = *cx.margins;
                cloned_prompt.clone().into_any_element()
            }),
            priority: 0,
        }];

        let focus_handle = bp_prompt.focus_handle(cx);
        window.focus(&focus_handle, cx);

        let block_ids = self.insert_blocks(blocks, None, cx);
        bp_prompt.update(cx, |prompt, _| {
            prompt.add_block_ids(block_ids);
        });
    }

    pub(crate) fn add_edit_breakpoint_block(
        &mut self,
        anchor: Anchor,
        breakpoint: &Breakpoint,
        edit_action: BreakpointPromptEditAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base_text: &str = match edit_action {
            BreakpointPromptEditAction::Log => breakpoint.message.as_ref(),
            BreakpointPromptEditAction::Condition => breakpoint.condition.as_ref(),
            BreakpointPromptEditAction::HitCondition => breakpoint.hit_condition.as_ref(),
        }
        .map(|msg| msg.as_ref())
        .unwrap_or_default();

        let placeholder_text = match edit_action {
            BreakpointPromptEditAction::Log => {
                "Message to log when a breakpoint is hit. Expressions within {} are interpolated."
            }
            BreakpointPromptEditAction::Condition => {
                "Condition when a breakpoint is hit. Expressions within {} are interpolated."
            }
            BreakpointPromptEditAction::HitCondition => "How many breakpoint hits to ignore",
        };

        let breakpoint = breakpoint.clone();
        self.add_edit_block(
            anchor,
            base_text,
            placeholder_text,
            Some(Box::new(move |message: String, editor: &mut Self, cx| {
                editor.edit_breakpoint_at_anchor(
                    anchor,
                    breakpoint,
                    match edit_action {
                        BreakpointPromptEditAction::Log => {
                            BreakpointEditAction::EditLogMessage(message.into())
                        }
                        BreakpointPromptEditAction::Condition => {
                            BreakpointEditAction::EditCondition(message.into())
                        }
                        BreakpointPromptEditAction::HitCondition => {
                            BreakpointEditAction::EditHitCondition(message.into())
                        }
                    },
                    cx,
                );
            })),
            None,
            window,
            cx,
        );
    }

    pub(crate) fn breakpoint_at_row(
        &self,
        row: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Anchor, Breakpoint)> {
        let snapshot = self.snapshot(window, cx);
        let breakpoint_position = snapshot.buffer_snapshot().anchor_before(Point::new(row, 0));

        self.breakpoint_at_anchor(breakpoint_position, &snapshot, cx)
    }

    pub(crate) fn breakpoint_at_anchor(
        &self,
        breakpoint_position: Anchor,
        snapshot: &EditorSnapshot,
        cx: &mut Context<Self>,
    ) -> Option<(Anchor, Breakpoint)> {
        let (breakpoint_position, _) = snapshot
            .buffer_snapshot()
            .anchor_to_buffer_anchor(breakpoint_position)?;
        let buffer = self.buffer.read(cx).buffer(breakpoint_position.buffer_id)?;

        let buffer_snapshot = buffer.read(cx).snapshot();

        let row = buffer_snapshot
            .summary_for_anchor::<text::PointUtf16>(&breakpoint_position)
            .row;

        let line_len = buffer_snapshot.line_len(row);
        let anchor_end = buffer_snapshot.anchor_after(Point::new(row, line_len));

        self.breakpoint_store
            .as_ref()?
            .read_with(cx, |breakpoint_store, cx| {
                breakpoint_store
                    .breakpoints(
                        &buffer,
                        Some(breakpoint_position..anchor_end),
                        &buffer_snapshot,
                        cx,
                    )
                    .next()
                    .and_then(|(bp, _)| {
                        let breakpoint_row = buffer_snapshot
                            .summary_for_anchor::<text::PointUtf16>(&bp.position)
                            .row;

                        if breakpoint_row == row {
                            snapshot
                                .buffer_snapshot()
                                .anchor_in_excerpt(bp.position)
                                .map(|position| (position, bp.bp.clone()))
                        } else {
                            None
                        }
                    })
            })
    }

    pub fn edit_log_breakpoint(
        &mut self,
        _: &EditLogBreakpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.breakpoint_store.is_none() {
            return;
        }

        for (anchor, breakpoint) in self.breakpoints_at_cursors(window, cx) {
            let breakpoint = breakpoint.unwrap_or_else(|| Breakpoint {
                message: None,
                state: BreakpointState::Enabled,
                condition: None,
                hit_condition: None,
            });

            self.add_edit_breakpoint_block(
                anchor,
                &breakpoint,
                BreakpointPromptEditAction::Log,
                window,
                cx,
            );
        }
    }

    pub(crate) fn breakpoints_at_cursors(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<(Anchor, Option<Breakpoint>)> {
        let snapshot = self.snapshot(window, cx);
        let cursors = self
            .selections
            .disjoint_anchors_arc()
            .iter()
            .map(|selection| {
                let cursor_position: Point = selection.head().to_point(&snapshot.buffer_snapshot());

                let breakpoint_position = self
                    .breakpoint_at_row(cursor_position.row, window, cx)
                    .map(|bp| bp.0)
                    .unwrap_or_else(|| {
                        snapshot
                            .display_snapshot
                            .buffer_snapshot()
                            .anchor_after(Point::new(cursor_position.row, 0))
                    });

                let breakpoint = self
                    .breakpoint_at_anchor(breakpoint_position, &snapshot, cx)
                    .map(|(anchor, breakpoint)| (anchor, Some(breakpoint)));

                breakpoint.unwrap_or_else(|| (breakpoint_position, None))
            })
            .collect::<HashMap<Anchor, _>>();

        cursors.into_iter().collect()
    }

    pub fn enable_breakpoint(
        &mut self,
        _: &crate::actions::EnableBreakpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.breakpoint_store.is_none() {
            return;
        }

        for (anchor, breakpoint) in self.breakpoints_at_cursors(window, cx) {
            let Some(breakpoint) = breakpoint.filter(|breakpoint| breakpoint.is_disabled()) else {
                continue;
            };
            self.edit_breakpoint_at_anchor(
                anchor,
                breakpoint,
                BreakpointEditAction::InvertState,
                cx,
            );
        }
    }

    pub fn disable_breakpoint(
        &mut self,
        _: &crate::actions::DisableBreakpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.breakpoint_store.is_none() {
            return;
        }

        for (anchor, breakpoint) in self.breakpoints_at_cursors(window, cx) {
            let Some(breakpoint) = breakpoint.filter(|breakpoint| breakpoint.is_enabled()) else {
                continue;
            };
            self.edit_breakpoint_at_anchor(
                anchor,
                breakpoint,
                BreakpointEditAction::InvertState,
                cx,
            );
        }
    }

    pub fn toggle_breakpoint(
        &mut self,
        _: &crate::actions::ToggleBreakpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.breakpoint_store.is_none() {
            return;
        }

        for (anchor, breakpoint) in self.breakpoints_at_cursors(window, cx) {
            if let Some(breakpoint) = breakpoint {
                self.edit_breakpoint_at_anchor(
                    anchor,
                    breakpoint,
                    BreakpointEditAction::Toggle,
                    cx,
                );
            } else {
                self.edit_breakpoint_at_anchor(
                    anchor,
                    Breakpoint::new_standard(),
                    BreakpointEditAction::Toggle,
                    cx,
                );
            }
        }
    }

    pub fn edit_breakpoint_at_anchor(
        &mut self,
        breakpoint_position: Anchor,
        breakpoint: Breakpoint,
        edit_action: BreakpointEditAction,
        cx: &mut Context<Self>,
    ) {
        let Some(breakpoint_store) = &self.breakpoint_store else {
            return;
        };
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let Some((position, _)) = buffer_snapshot.anchor_to_buffer_anchor(breakpoint_position)
        else {
            return;
        };
        let Some(buffer) = self.buffer.read(cx).buffer(position.buffer_id) else {
            return;
        };

        breakpoint_store.update(cx, |breakpoint_store, cx| {
            breakpoint_store.toggle_breakpoint(
                buffer,
                BreakpointWithPosition {
                    position,
                    bp: breakpoint,
                },
                edit_action,
                cx,
            );
        });

        cx.notify();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn breakpoint_store(&self) -> Option<Entity<BreakpointStore>> {
        self.breakpoint_store.clone()
    }

    pub(crate) fn go_to_active_debug_line(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        maybe!({
            let breakpoint_store = self.breakpoint_store.as_ref()?;

            let (active_stack_frame, debug_line_pane_id) = {
                let store = breakpoint_store.read(cx);
                let active_stack_frame = store.active_position().cloned();
                let debug_line_pane_id = store.active_debug_line_pane_id();
                (active_stack_frame, debug_line_pane_id)
            };

            let Some(active_stack_frame) = active_stack_frame else {
                self.clear_row_highlights::<ActiveDebugLine>();
                return None;
            };

            if let Some(debug_line_pane_id) = debug_line_pane_id {
                if let Some(workspace) = self
                    .workspace
                    .as_ref()
                    .and_then(|(workspace, _)| workspace.upgrade())
                {
                    let editor_pane_id = workspace
                        .read(cx)
                        .pane_for_item_id(cx.entity_id())
                        .map(|pane| pane.entity_id());

                    if editor_pane_id.is_some_and(|id| id != debug_line_pane_id) {
                        self.clear_row_highlights::<ActiveDebugLine>();
                        return None;
                    }
                }
            }

            let position = active_stack_frame.position;

            let snapshot = self.buffer.read(cx).snapshot(cx);
            let multibuffer_anchor = snapshot.anchor_in_excerpt(position)?;

            self.clear_row_highlights::<ActiveDebugLine>();

            self.go_to_line::<ActiveDebugLine>(
                multibuffer_anchor,
                |cx| cx.theme().colors().editor_debugger_active_line_background,
                window,
                cx,
            );

            cx.notify();

            Some(())
        })
        .is_some()
    }
}
