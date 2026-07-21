use super::*;

impl ProjectPanel {
    pub(super) fn handle_panel_drag_move<T: 'static>(
        this: &mut ProjectPanel,
        e: &DragMoveEvent<T>,
        window: &mut Window,
        cx: &mut Context<ProjectPanel>,
    ) {
        if let Some(previous_position) = this.previous_drag_position {
            // Refresh cursor only when an actual drag happens,
            // because modifiers are not updated when the cursor is not moved.
            if e.event.position != previous_position {
                this.refresh_drag_cursor_style(&e.event.modifiers, window, cx);
            }
        }
        this.previous_drag_position = Some(e.event.position);

        if !e.bounds.contains(&e.event.position) {
            this.drag_target_entry = None;
            return;
        }
        this.hover_scroll_task.take();
        let panel_height = e.bounds.size.height;
        if panel_height <= px(0.) {
            return;
        }

        let event_offset = e.event.position.y - e.bounds.origin.y;
        // How far along in the project panel is our cursor? (0. is the top of a list, 1. is the bottom)
        let hovered_region_offset = event_offset / panel_height;

        // We want the scrolling to be a bit faster when the cursor is closer to the edge of a list.
        // These pixels offsets were picked arbitrarily.
        let vertical_scroll_offset = if hovered_region_offset <= 0.05 {
            8.
        } else if hovered_region_offset <= 0.15 {
            5.
        } else if hovered_region_offset >= 0.95 {
            -8.
        } else if hovered_region_offset >= 0.85 {
            -5.
        } else {
            return;
        };
        let adjustment = point(px(0.), px(vertical_scroll_offset));
        this.hover_scroll_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let should_stop_scrolling = this
                    .update(cx, |this, cx| {
                        this.hover_scroll_task.as_ref()?;
                        let handle = this.scroll_handle.0.borrow_mut();
                        let offset = handle.base_handle.offset();

                        handle.base_handle.set_offset(offset + adjustment);
                        cx.notify();
                        Some(())
                    })
                    .ok()
                    .flatten()
                    .is_some();
                if should_stop_scrolling {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        }));
    }
}
