use super::*;

impl SplittableEditor {
    pub fn toggle_split(
        &mut self,
        _: &ToggleSplitDiff,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.diff_view_style {
            DiffViewStyle::Unified => {
                self.diff_view_style = DiffViewStyle::Split;
                if !self.too_narrow_for_split {
                    self.split(window, cx);
                }
            }
            DiffViewStyle::Split => {
                self.diff_view_style = DiffViewStyle::Unified;
                if self.is_split() {
                    self.unsplit(window, cx);
                }
            }
        }
    }

    pub(crate) fn intercept_toggle_breakpoint(
        &mut self,
        _: &ToggleBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn intercept_enable_breakpoint(
        &mut self,
        _: &EnableBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn intercept_disable_breakpoint(
        &mut self,
        _: &DisableBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn intercept_edit_log_breakpoint(
        &mut self,
        _: &EditLogBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn intercept_inline_assist(
        &mut self,
        _: &InlineAssist,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lhs.is_some() {
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn toggle_soft_wrap(
        &mut self,
        _: &ToggleSoftWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            cx.stop_propagation();

            let is_lhs_focused = lhs.was_last_focused;
            let (focused_editor, other_editor) = if is_lhs_focused {
                (&lhs.editor, &self.rhs_editor)
            } else {
                (&self.rhs_editor, &lhs.editor)
            };

            // Toggle the focused editor
            focused_editor.update(cx, |editor, cx| {
                editor.toggle_soft_wrap(&ToggleSoftWrap, window, cx);
            });

            // Copy the soft wrap state from the focused editor to the other editor
            let soft_wrap_override = focused_editor.read(cx).soft_wrap_mode_override;
            other_editor.update(cx, |editor, cx| {
                editor.soft_wrap_mode_override = soft_wrap_override;
                cx.notify();
            });
        } else {
            cx.propagate();
        }
    }

    pub(crate) fn unsplit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(lhs) = self.lhs.take() else {
            return;
        };

        // Detach the stale lhs editor from the shared scroll anchor while the split companion still exists,
        // so its anchor can be converted to lhs native before rhs tears down split specific state.
        lhs.editor.update(cx, |editor, cx| {
            let lhs_snapshot = editor.display_map.update(cx, |dm, cx| dm.snapshot(cx));
            editor
                .scroll_manager
                .unshare_scroll_anchor(&lhs_snapshot, cx);
            editor.set_on_local_selections_changed(None);
        });

        self.rhs_editor.update(cx, |rhs, cx| {
            let rhs_snapshot = rhs.display_map.update(cx, |dm, cx| dm.snapshot(cx));
            let native_anchor = rhs.scroll_manager.native_anchor(&rhs_snapshot, cx);
            let rhs_display_map_id = rhs_snapshot.display_map_id;
            rhs.scroll_manager
                .scroll_anchor_entity()
                .update(cx, |shared, _| {
                    shared.scroll_anchor = native_anchor;
                    shared.display_map_id = Some(rhs_display_map_id);
                });

            rhs.set_on_local_selections_changed(None);
            rhs.set_delegate_expand_excerpts(false);
            rhs.buffer().update(cx, |buffer, cx| {
                buffer.set_show_deleted_hunks(true, cx);
                buffer.set_use_extended_diff_range(false, cx);
            });
            rhs.display_map.update(cx, |dm, cx| {
                dm.set_companion(None, cx);
            });
        });
        cx.notify();
    }
}
