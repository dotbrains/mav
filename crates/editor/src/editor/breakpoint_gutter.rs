use super::*;

impl Editor {
    pub(crate) fn active_breakpoints(
        &self,
        range: Range<DisplayRow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HashMap<DisplayRow, (Anchor, Breakpoint, Option<BreakpointSessionState>)> {
        let mut breakpoint_display_points = HashMap::default();

        let Some(breakpoint_store) = self.breakpoint_store.clone() else {
            return breakpoint_display_points;
        };

        let snapshot = self.snapshot(window, cx);
        let multi_buffer_snapshot = snapshot.buffer_snapshot();

        let range = snapshot.display_point_to_point(DisplayPoint::new(range.start, 0), Bias::Left)
            ..snapshot.display_point_to_point(DisplayPoint::new(range.end, 0), Bias::Right);

        for (buffer_snapshot, range, _) in
            multi_buffer_snapshot.range_to_buffer_ranges(range.start..range.end)
        {
            let Some(buffer) = self.buffer().read(cx).buffer(buffer_snapshot.remote_id()) else {
                continue;
            };
            let breakpoints = breakpoint_store.read(cx).breakpoints(
                &buffer,
                Some(
                    buffer_snapshot.anchor_before(range.start)
                        ..buffer_snapshot.anchor_after(range.end),
                ),
                &buffer_snapshot,
                cx,
            );
            for (breakpoint, state) in breakpoints {
                let Some(multi_buffer_anchor) =
                    multi_buffer_snapshot.anchor_in_excerpt(breakpoint.position)
                else {
                    continue;
                };
                let position = multi_buffer_anchor
                    .to_point(&multi_buffer_snapshot)
                    .to_display_point(&snapshot);

                breakpoint_display_points.insert(
                    position.row(),
                    (multi_buffer_anchor, breakpoint.bp.clone(), state),
                );
            }
        }

        breakpoint_display_points
    }

    pub(crate) fn render_breakpoint(
        &self,
        position: Anchor,
        row: DisplayRow,
        breakpoint: &Breakpoint,
        state: Option<BreakpointSessionState>,
        cx: &mut Context<Self>,
    ) -> IconButton {
        let is_rejected = state.is_some_and(|s| !s.verified);

        let icon = match (&breakpoint.message.is_some(), breakpoint.is_disabled()) {
            (false, false) => ui::IconName::DebugBreakpoint,
            (true, false) => ui::IconName::DebugLogBreakpoint,
            (false, true) => ui::IconName::DebugDisabledBreakpoint,
            (true, true) => ui::IconName::DebugDisabledLogBreakpoint,
        };
        let color = if is_rejected {
            Color::Disabled
        } else {
            Color::Debugger
        };

        let breakpoint = Arc::from(breakpoint.clone());
        let alt_as_text = gpui::Keystroke {
            modifiers: Modifiers::secondary_key(),
            ..Default::default()
        };
        let primary_action_text = "Unset breakpoint";
        let focus_handle = self.focus_handle.clone();
        let has_context_menu = self.has_mouse_context_menu();

        let meta = if is_rejected {
            SharedString::from("No executable code is associated with this line.")
        } else if !breakpoint.is_disabled() {
            SharedString::from(format!(
                "{alt_as_text}-click to disable\nright-click for more options"
            ))
        } else {
            SharedString::from("Right-click for more options")
        };

        IconButton::new(("breakpoint_indicator", row.0 as usize), icon)
            .icon_size(IconSize::XSmall)
            .size(ui::ButtonSize::None)
            .when(is_rejected, |this| {
                this.indicator(Indicator::icon(Icon::new(IconName::Warning)).color(Color::Warning))
            })
            .icon_color(color)
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener({
                move |editor, event: &ClickEvent, window, cx| {
                    let edit_action = if event.modifiers().platform || breakpoint.is_disabled() {
                        BreakpointEditAction::InvertState
                    } else {
                        BreakpointEditAction::Toggle
                    };

                    window.focus(&editor.focus_handle(cx), cx);
                    editor.edit_breakpoint_at_anchor(
                        position,
                        breakpoint.as_ref().clone(),
                        edit_action,
                        cx,
                    );
                }
            }))
            .on_right_click(cx.listener(move |editor, event: &ClickEvent, window, cx| {
                editor.set_gutter_context_menu(row, Some(position), event.position(), window, cx);
            }))
            .when(!has_context_menu, |button| {
                button.tooltip(move |_window, cx| {
                    Tooltip::with_meta_in(
                        primary_action_text,
                        Some(&ToggleBreakpoint),
                        meta.clone(),
                        &focus_handle,
                        cx,
                    )
                })
            })
    }
}
