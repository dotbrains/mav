mod buffer_header;
mod sticky_paint;

use std::path::Path;
use std::rc::Rc;

use collections::HashMap;
use file_icons::FileIcons;
use git::status::FileStatus;
use gpui::{
    Action, AnyElement, App, AvailableSpace, Bounds, ClickEvent, ClipboardItem, ContentMask,
    CursorStyle, DefiniteLength, Entity, Focusable as _, Hitbox, HitboxBehavior, Hsla, IntoElement,
    Length, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels,
    ShapedLine, SharedString, Styled, TextAlign, Window, WindowBackgroundAppearance, div, fill,
    linear_color_stop, linear_gradient, point, px, size,
};
use language::language_settings::ShowWhitespaceSetting;
use multi_buffer::{Anchor, ExcerptBoundaryInfo};
use project::Entry;
use settings::{RelativeLineNumbers, Settings};
use smallvec::SmallVec;
use sum_tree::Bias;
use text::BufferId;
use theme::ActiveTheme;
use ui::{
    ButtonLike, ContextMenu, Indicator, KeyBinding, Tooltip, prelude::*, right_click_menu,
    text_for_keystroke,
};
use util::ResultExt;
use workspace::{ItemHandle, ItemSettings, OpenInTerminal, OpenTerminal, RevealInProjectPanel};

use super::{
    BlockLayout, EditorElement, EditorLayout, LineWithInvisibles, layout_line,
    render_breadcrumb_text,
};
use crate::{
    BUFFER_HEADER_PADDING, DisplayRow, Editor, EditorSettings, EditorSnapshot, FILE_HEADER_HEIGHT,
    GutterDimensions, JumpData, MULTI_BUFFER_EXCERPT_HEADER_HEIGHT, OpenExcerpts, Point, RowExt,
    SelectionEffects, StickyHeaderExcerpt, ToPoint, ToggleFold, ToggleFoldAll,
    display_map::ToDisplayPoint,
    scroll::{Autoscroll, ScrollOffset, ScrollPixelOffset},
};

pub(crate) use buffer_header::render_buffer_header;

pub(crate) struct StickyHeader {
    sticky_row: DisplayRow,
    pub(crate) start_point: Point,
    pub(crate) offset: ScrollOffset,
}

pub(super) struct StickyHeaders {
    pub(super) lines: Vec<StickyHeaderLine>,
    gutter_background: Hsla,
    content_background: Hsla,
    gutter_right_padding: Pixels,
}

pub(super) struct StickyHeaderLine {
    row: DisplayRow,
    pub(super) offset: Pixels,
    line: Rc<LineWithInvisibles>,
    line_number: Option<ShapedLine>,
    elements: SmallVec<[AnyElement; 1]>,
    available_text_width: Pixels,
    hitbox: Hitbox,
}

impl EditorElement {
    pub(crate) fn sticky_headers(editor: &Editor, snapshot: &EditorSnapshot) -> Vec<StickyHeader> {
        let scroll_top = snapshot.scroll_position().y;

        let mut end_rows = Vec::<DisplayRow>::new();
        let mut rows = Vec::<StickyHeader>::new();

        for item in editor.sticky_headers.iter().flatten() {
            let selection_start = item
                .selection_range
                .start
                .to_point(snapshot.buffer_snapshot());
            let source_text_start = item
                .source_range_for_text
                .start
                .to_point(snapshot.buffer_snapshot());
            let start_column = if source_text_start.row == selection_start.row {
                source_text_start.column
            } else {
                0
            };
            let start_point = Point::new(selection_start.row, start_column);
            let end_point = item.range.end.to_point(snapshot.buffer_snapshot());

            let sticky_row = snapshot
                .display_snapshot
                .point_to_display_point(start_point, Bias::Left)
                .row();
            if rows
                .last()
                .is_some_and(|last| last.sticky_row == sticky_row)
            {
                continue;
            }

            let end_row = snapshot
                .display_snapshot
                .point_to_display_point(end_point, Bias::Left)
                .row();
            let max_sticky_row = end_row.previous_row();
            if max_sticky_row <= sticky_row {
                continue;
            }

            while end_rows
                .last()
                .is_some_and(|&last_end| last_end <= sticky_row)
            {
                end_rows.pop();
            }
            let depth = end_rows.len();
            let adjusted_scroll_top = scroll_top + depth as f64;

            if sticky_row.as_f64() >= adjusted_scroll_top || end_row.as_f64() <= adjusted_scroll_top
            {
                continue;
            }

            let max_scroll_offset = max_sticky_row.as_f64() - scroll_top;
            let offset = (depth as f64).min(max_scroll_offset);

            end_rows.push(end_row);
            rows.push(StickyHeader {
                sticky_row,
                start_point,
                offset,
            });
        }

        rows
    }

    pub(super) fn should_show_buffer_headers(&self) -> bool {
        self.split_side.is_none()
    }

    pub(super) fn layout_sticky_buffer_header(
        &self,
        StickyHeaderExcerpt { excerpt }: StickyHeaderExcerpt<'_>,
        scroll_position: gpui::Point<ScrollOffset>,
        line_height: Pixels,
        right_margin: Pixels,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        selected_buffer_ids: &Vec<BufferId>,
        blocks: &[BlockLayout],
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let jump_data = header_jump_data(
            snapshot,
            DisplayRow(scroll_position.y as u32),
            FILE_HEADER_HEIGHT + MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
            excerpt,
            latest_selection_anchors,
        );

        let editor_bg_color = cx.theme().colors().editor_background;

        let selected = selected_buffer_ids.contains(&excerpt.buffer_id());

        let available_width = hitbox.bounds.size.width - right_margin;

        let mut header = v_flex()
            .w_full()
            .relative()
            .child(
                div()
                    .w(available_width)
                    .h(FILE_HEADER_HEIGHT as f32 * line_height)
                    .bg(linear_gradient(
                        0.,
                        linear_color_stop(editor_bg_color.opacity(0.), 0.),
                        linear_color_stop(editor_bg_color, 0.6),
                    ))
                    .absolute()
                    .top_0(),
            )
            .child(
                render_buffer_header(
                    &self.editor,
                    excerpt,
                    false,
                    selected,
                    true,
                    jump_data,
                    window,
                    cx,
                )
                .into_any_element(),
            )
            .into_any_element();

        let mut origin = hitbox.origin;
        // Move floating header up to avoid colliding with the next buffer header.
        for block in blocks.iter() {
            if !block.is_buffer_header {
                continue;
            }

            let Some(display_row) = block.row.filter(|row| row.0 > scroll_position.y as u32) else {
                continue;
            };

            let max_row = display_row.0.saturating_sub(FILE_HEADER_HEIGHT);
            let offset = scroll_position.y - max_row as f64;

            if offset > 0.0 {
                origin.y -= Pixels::from(offset * ScrollPixelOffset::from(line_height));
            }
            break;
        }

        let size = size(
            AvailableSpace::Definite(available_width),
            AvailableSpace::MinContent,
        );

        header.prepaint_as_root(origin, size, window, cx);

        header
    }

    pub(super) fn layout_sticky_headers(
        &self,
        snapshot: &EditorSnapshot,
        editor_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        gutter_dimensions: &GutterDimensions,
        gutter_hitbox: &Hitbox,
        text_hitbox: &Hitbox,
        relative_line_numbers: RelativeLineNumbers,
        relative_to: Option<DisplayRow>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<StickyHeaders> {
        let show_line_numbers = snapshot
            .show_line_numbers
            .unwrap_or_else(|| EditorSettings::get_global(cx).gutter.line_numbers);

        let rows = Self::sticky_headers(self.editor.read(cx), snapshot);

        let mut lines = Vec::<StickyHeaderLine>::new();

        for StickyHeader {
            sticky_row,
            start_point,
            offset,
        } in rows.into_iter().rev()
        {
            let line = layout_line(
                sticky_row,
                snapshot,
                &self.style,
                editor_width,
                is_row_soft_wrapped,
                window,
                cx,
            );

            let line_number = show_line_numbers.then(|| {
                let start_display_row = start_point.to_display_point(snapshot).row();
                let relative_number = relative_to
                    .filter(|_| relative_line_numbers != RelativeLineNumbers::Disabled)
                    .map(|base| {
                        snapshot.relative_line_delta(
                            base,
                            start_display_row,
                            relative_line_numbers == RelativeLineNumbers::Wrapped,
                        )
                    });
                let number = relative_number
                    .filter(|&delta| delta != 0)
                    .map(|delta| delta.unsigned_abs() as u32)
                    .unwrap_or(start_point.row + 1);
                let color = cx.theme().colors().editor_line_number;
                self.shape_line_number(SharedString::from(number.to_string()), color, window)
            });

            lines.push(StickyHeaderLine::new(
                sticky_row,
                line_height * offset as f32,
                line,
                line_number,
                line_height,
                scroll_pixel_position,
                content_origin,
                gutter_hitbox,
                text_hitbox,
                window,
                cx,
            ));
        }

        lines.reverse();
        if lines.is_empty() {
            return None;
        }

        Some(StickyHeaders {
            lines,
            gutter_background: cx.theme().colors().editor_gutter_background,
            content_background: self.style.background,
            gutter_right_padding: gutter_dimensions.right_padding,
        })
    }

    pub(super) fn paint_sticky_headers(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(mut sticky_headers) = layout.sticky_headers.take() else {
            return;
        };

        let Some(last_line_offset) = sticky_headers.lines.last().map(|line| line.offset) else {
            layout.sticky_headers = Some(sticky_headers);
            return;
        };

        let whitespace_setting = self
            .editor
            .read(cx)
            .buffer
            .read(cx)
            .language_settings(cx)
            .show_whitespaces;
        sticky_headers.paint(layout, whitespace_setting, window, cx);

        let sticky_header_hitboxes: Vec<Hitbox> = sticky_headers
            .lines
            .iter()
            .map(|line| line.hitbox.clone())
            .collect();
        let hovered_hitbox = sticky_header_hitboxes
            .iter()
            .find_map(|hitbox| hitbox.is_hovered(window).then_some(hitbox.id));

        window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, _cx| {
            if !phase.bubble() {
                return;
            }

            let current_hover = sticky_header_hitboxes
                .iter()
                .find_map(|hitbox| hitbox.is_hovered(window).then_some(hitbox.id));
            if hovered_hitbox != current_hover {
                window.refresh();
            }
        });

        let position_map = layout.position_map.clone();

        for (line_index, line) in sticky_headers.lines.iter().enumerate() {
            let editor = self.editor.clone();
            let hitbox = line.hitbox.clone();
            let row = line.row;
            let line_layout = line.line.clone();
            let position_map = position_map.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if !phase.bubble() {
                    return;
                }

                if event.button == MouseButton::Left && hitbox.is_hovered(window) {
                    let point_for_position =
                        position_map.point_for_position_on_line(event.position, row, &line_layout);

                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.snapshot(window, cx);
                        let anchor = snapshot
                            .display_snapshot
                            .display_point_to_anchor(point_for_position.nearest_valid, Bias::Left);
                        editor.change_selections(
                            SelectionEffects::scroll(Autoscroll::top_relative(
                                line_index as ScrollOffset,
                            )),
                            window,
                            cx,
                            |selections| {
                                selections.clear_disjoint();
                                selections.set_pending_anchor_range(
                                    anchor..anchor,
                                    crate::SelectMode::Character,
                                );
                            },
                        );
                        cx.stop_propagation();
                    });
                }
            });
        }

        let text_bounds = layout.position_map.text_hitbox.bounds;
        let border_top = text_bounds.top() + last_line_offset + layout.position_map.line_height;
        let separator_height = px(1.);
        let border_bounds = window.pixel_snap_bounds(Bounds::from_corners(
            point(layout.gutter_hitbox.bounds.left(), border_top),
            point(text_bounds.right(), border_top + separator_height),
        ));
        window.paint_quad(fill(border_bounds, cx.theme().colors().border_variant));

        layout.sticky_headers = Some(sticky_headers);
    }
}

pub(crate) fn header_jump_data(
    editor_snapshot: &EditorSnapshot,
    block_row_start: DisplayRow,
    height: u32,
    first_excerpt: &ExcerptBoundaryInfo,
    latest_selection_anchors: &HashMap<BufferId, Anchor>,
) -> JumpData {
    let multibuffer_snapshot = editor_snapshot.buffer_snapshot();
    let buffer = first_excerpt.buffer(multibuffer_snapshot);
    let (jump_anchor, jump_buffer, excerpt_start) = if let Some(anchor) =
        latest_selection_anchors.get(&first_excerpt.buffer_id())
        && let Some((jump_anchor, selection_buffer)) =
            multibuffer_snapshot.anchor_to_buffer_anchor(*anchor)
    {
        let jump_offset = text::ToOffset::to_offset(&jump_anchor, selection_buffer);
        let selection_excerpt_start = multibuffer_snapshot
            .excerpts_for_buffer(jump_anchor.buffer_id)
            .find(|excerpt| {
                let start = text::ToOffset::to_offset(&excerpt.context.start, selection_buffer);
                let end = text::ToOffset::to_offset(&excerpt.context.end, selection_buffer);
                start <= jump_offset && jump_offset <= end
            })
            .map(|excerpt| excerpt.context.start)
            .unwrap_or(first_excerpt.range.context.start);
        (jump_anchor, selection_buffer, selection_excerpt_start)
    } else {
        (
            first_excerpt.range.primary.start,
            buffer,
            first_excerpt.range.context.start,
        )
    };
    let jump_position = language::ToPoint::to_point(&jump_anchor, jump_buffer);
    let rows_from_excerpt_start = if jump_anchor == excerpt_start {
        0
    } else {
        let excerpt_start_point = language::ToPoint::to_point(&excerpt_start, jump_buffer);
        jump_position.row.saturating_sub(excerpt_start_point.row)
    };

    let line_offset_from_top = (block_row_start.0 + height + rows_from_excerpt_start)
        .saturating_sub(
            editor_snapshot
                .scroll_anchor
                .scroll_position(&editor_snapshot.display_snapshot)
                .y as u32,
        );

    JumpData::MultiBufferPoint {
        anchor: jump_anchor,
        position: jump_position,
        line_offset_from_top,
    }
}
