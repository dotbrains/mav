use std::rc::Rc;

use gpui::{AppContext as _, Empty, EntityId, Stateful, StatefulInteractiveElement};

use crate::{
    ActiveTheme as _, AnyElement, App, Div, FluentBuilder as _, InteractiveElement, IntoElement,
    ParentElement, Styled, Window, div, px,
};

use super::{DraggedColumn, RESIZE_COLUMN_WIDTH, RESIZE_DIVIDER_WIDTH};

/// Builds a single column resize divider with an interactive drag handle.
pub(crate) fn render_column_resize_divider(
    divider: Stateful<Div>,
    col_idx: usize,
    is_resizable: bool,
    entity_id: EntityId,
    on_reset: Rc<dyn Fn(&mut Window, &mut App)>,
    on_drag_end: Option<Rc<dyn Fn(&mut App)>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    window.with_id(col_idx, |window| {
        let mut resize_divider = divider.w(px(RESIZE_DIVIDER_WIDTH)).h_full().bg(cx
            .theme()
            .colors()
            .border
            .opacity(0.8));

        let mut resize_handle = div()
            .id("column-resize-handle")
            .absolute()
            .left_neg_0p5()
            .w(px(RESIZE_COLUMN_WIDTH))
            .h_full();

        if is_resizable {
            let is_highlighted = window.use_state(cx, |_window, _cx| false);

            resize_divider = resize_divider.when(*is_highlighted.read(cx), |div| {
                div.bg(cx.theme().colors().border_focused)
            });

            resize_handle = resize_handle
                .on_hover({
                    let is_highlighted = is_highlighted.clone();
                    move |&was_hovered, _, cx| is_highlighted.write(cx, was_hovered)
                })
                .cursor_col_resize()
                .on_click(move |event, window, cx| {
                    if event.click_count() >= 2 {
                        on_reset(window, cx);
                    }
                    cx.stop_propagation();
                })
                .on_drag(
                    DraggedColumn {
                        col_idx,
                        state_id: entity_id,
                    },
                    {
                        let is_highlighted = is_highlighted.clone();
                        move |_, _offset, _window, cx| {
                            is_highlighted.write(cx, true);
                            cx.new(|_cx| Empty)
                        }
                    },
                )
                .on_drop::<DraggedColumn>(move |_, _, cx| {
                    is_highlighted.write(cx, false);
                    if let Some(on_drag_end) = &on_drag_end {
                        on_drag_end(cx);
                    }
                });
        }

        resize_divider.child(resize_handle).into_any_element()
    })
}
