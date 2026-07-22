use super::*;

impl MemoryView {
    pub(super) fn render_memory(&self, cx: &mut Context<Self>) -> UniformList {
        let weak = cx.weak_entity();
        let session = self.session.clone();
        let view_state = self.view_state_handle.0.borrow().clone();
        uniform_list(
            "debugger-memory-view",
            view_state.row_count() as usize,
            move |range, _, cx| {
                let mut line_buffer = Vec::with_capacity(view_state.line_width.width as usize);
                let memory_start =
                    (view_state.base_row + range.start as u64) * view_state.line_width.width as u64;
                let memory_end = (view_state.base_row + range.end as u64)
                    * view_state.line_width.width as u64
                    - 1;
                let mut memory = session.update(cx, |this, cx| {
                    this.read_memory(memory_start..=memory_end, cx)
                });
                let mut rows = Vec::with_capacity(range.end - range.start);
                for ix in range {
                    line_buffer.extend((&mut memory).take(view_state.line_width.width as usize));
                    rows.push(render_single_memory_view_line(
                        &line_buffer,
                        ix as u64,
                        weak.clone(),
                        cx,
                    ));
                    line_buffer.clear();
                }
                rows
            },
        )
        .track_scroll(&view_state.scroll_handle)
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .on_scroll_wheel(cx.listener(|this, evt: &ScrollWheelEvent, window, _| {
            let mut view_state = this.view_state();
            let delta = evt.delta.pixel_delta(window.line_height());
            let current_offset = view_state.scroll_handle.offset();
            view_state
                .set_offset(current_offset.apply_along(Axis::Vertical, |offset| offset + delta.y));
        }))
    }
}

pub(super) fn render_single_memory_view_line(
    memory: &[MemoryCell],
    ix: u64,
    weak: gpui::WeakEntity<MemoryView>,
    cx: &mut App,
) -> AnyElement {
    let Ok(view_state) = weak.update(cx, |this, _| this.view_state().clone()) else {
        return div().into_any();
    };
    let base_address = (view_state.base_row + ix) * view_state.line_width.width as u64;

    h_flex()
        .id((
            "memory-view-row-full",
            ix * view_state.line_width.width as u64,
        ))
        .size_full()
        .gap_x_2()
        .child(
            div()
                .child(
                    Label::new(format!("{:016X}", base_address))
                        .buffer_font(cx)
                        .size(ui::LabelSize::Small)
                        .color(Color::Muted),
                )
                .px_1()
                .border_r_1()
                .border_color(Color::Muted.color(cx)),
        )
        .child(
            h_flex()
                .id((
                    "memory-view-row-raw-memory",
                    ix * view_state.line_width.width as u64,
                ))
                .px_1()
                .children(memory.iter().enumerate().map(|(cell_ix, cell)| {
                    let weak = weak.clone();
                    div()
                        .id(("memory-view-row-raw-memory-cell", cell_ix as u64))
                        .px_0p5()
                        .when_some(view_state.selection.as_ref(), |this, selection| {
                            this.when(selection.contains(base_address + cell_ix as u64), |this| {
                                let weak = weak.clone();

                                this.bg(Color::Selected.color(cx).opacity(0.2)).when(
                                    !selection.is_dragging(),
                                    |this| {
                                        let selection = selection.drag().memory_range();
                                        this.on_mouse_down(
                                            MouseButton::Right,
                                            move |click, window, cx| {
                                                _ = weak.update(cx, |this, cx| {
                                                    this.deploy_memory_context_menu(
                                                        selection.clone(),
                                                        click.position,
                                                        window,
                                                        cx,
                                                    )
                                                });
                                                cx.stop_propagation();
                                            },
                                        )
                                    },
                                )
                            })
                        })
                        .child(
                            Label::new(
                                cell.0
                                    .map(|val| HEX_BYTES_MEMOIMAV[val as usize].clone())
                                    .unwrap_or_else(|| UNKNOWN_BYTE.clone()),
                            )
                            .buffer_font(cx)
                            .when(cell.0.is_none(), |this| this.color(Color::Muted))
                            .size(ui::LabelSize::Small),
                        )
                        .on_drag(
                            Drag {
                                start_address: base_address + cell_ix as u64,
                                end_address: base_address + cell_ix as u64,
                            },
                            {
                                let weak = weak.clone();
                                move |drag, _, _, cx| {
                                    _ = weak.update(cx, |this, _| {
                                        this.view_state().selection =
                                            Some(SelectedMemoryRange::DragUnderway(drag.clone()));
                                    });

                                    cx.new(|_| Empty)
                                }
                            },
                        )
                        .on_drop({
                            let weak = weak.clone();
                            move |drag: &Drag, _, cx| {
                                _ = weak.update(cx, |this, _| {
                                    this.view_state().selection =
                                        Some(SelectedMemoryRange::DragComplete(Drag {
                                            start_address: drag.start_address,
                                            end_address: base_address + cell_ix as u64,
                                        }));
                                });
                            }
                        })
                        .drag_over(move |style, drag: &Drag, _, cx| {
                            _ = weak.update(cx, |this, _| {
                                this.view_state().selection =
                                    Some(SelectedMemoryRange::DragUnderway(Drag {
                                        start_address: drag.start_address,
                                        end_address: base_address + cell_ix as u64,
                                    }));
                            });

                            style
                        })
                })),
        )
        .child(
            h_flex()
                .id((
                    "memory-view-row-ascii-memory",
                    ix * view_state.line_width.width as u64,
                ))
                .h_full()
                .px_1()
                .mr_4()
                // .gap_x_1p5()
                .border_x_1()
                .border_color(Color::Muted.color(cx))
                .children(memory.iter().enumerate().map(|(ix, cell)| {
                    let as_character = char::from(cell.0.unwrap_or(0));
                    let as_visible = if as_character.is_ascii_graphic() {
                        as_character
                    } else {
                        '·'
                    };
                    div()
                        .px_0p5()
                        .when_some(view_state.selection.as_ref(), |this, selection| {
                            this.when(selection.contains(base_address + ix as u64), |this| {
                                this.bg(Color::Selected.color(cx).opacity(0.2))
                            })
                        })
                        .child(
                            Label::new(format!("{as_visible}"))
                                .buffer_font(cx)
                                .when(cell.0.is_none(), |this| this.color(Color::Muted))
                                .size(ui::LabelSize::Small),
                        )
                })),
        )
        .into_any()
}
