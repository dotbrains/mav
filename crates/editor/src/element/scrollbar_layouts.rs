use super::*;

#[derive(Clone)]
pub(super) struct EditorScrollbars {
    pub vertical: Option<ScrollbarLayout>,
    pub horizontal: Option<ScrollbarLayout>,
    pub visible: bool,
}

impl EditorScrollbars {
    pub fn from_scrollbar_axes(
        show_scrollbar: ScrollbarAxes,
        layout_information: &ScrollbarLayoutInformation,
        content_offset: gpui::Point<Pixels>,
        scroll_position: gpui::Point<f64>,
        scrollbar_width: Pixels,
        right_margin: Pixels,
        editor_width: Pixels,
        show_scrollbars: bool,
        scrollbar_state: Option<&ActiveScrollbarState>,
        window: &mut Window,
    ) -> Self {
        let ScrollbarLayoutInformation {
            editor_bounds,
            scroll_range,
            glyph_grid_cell,
        } = layout_information;

        let viewport_size = size(editor_width, editor_bounds.size.height);

        let scrollbar_bounds_for = |axis: ScrollbarAxis| match axis {
            ScrollbarAxis::Horizontal => Bounds::from_anchor_and_size(
                gpui::Anchor::BottomLeft,
                editor_bounds.bottom_left(),
                size(editor_bounds.size.width - right_margin, scrollbar_width),
            ),
            ScrollbarAxis::Vertical => Bounds::from_anchor_and_size(
                gpui::Anchor::TopRight,
                editor_bounds.top_right(),
                size(scrollbar_width, viewport_size.height),
            ),
        };

        let mut create_scrollbar_layout = |axis| {
            let viewport_size = viewport_size.along(axis);
            let scroll_range = scroll_range.along(axis);

            (show_scrollbar.along(axis)
                && (axis == ScrollbarAxis::Vertical || scroll_range > viewport_size))
                .then(|| {
                    ScrollbarLayout::new(
                        window.insert_hitbox(scrollbar_bounds_for(axis), HitboxBehavior::Normal),
                        viewport_size,
                        scroll_range,
                        glyph_grid_cell.along(axis),
                        content_offset.along(axis),
                        scroll_position.along(axis),
                        show_scrollbars,
                        axis,
                    )
                    .with_thumb_state(
                        scrollbar_state.and_then(|state| state.thumb_state_for_axis(axis)),
                    )
                })
        };

        Self {
            vertical: create_scrollbar_layout(ScrollbarAxis::Vertical),
            horizontal: create_scrollbar_layout(ScrollbarAxis::Horizontal),
            visible: show_scrollbars,
        }
    }

    pub fn iter_scrollbars(&self) -> impl Iterator<Item = (&ScrollbarLayout, ScrollbarAxis)> + '_ {
        [
            (&self.vertical, ScrollbarAxis::Vertical),
            (&self.horizontal, ScrollbarAxis::Horizontal),
        ]
        .into_iter()
        .filter_map(|(scrollbar, axis)| scrollbar.as_ref().map(|s| (s, axis)))
    }

    pub fn get_hovered_axis(&self, window: &Window) -> Option<(&ScrollbarLayout, ScrollbarAxis)> {
        self.iter_scrollbars()
            .find(|s| s.0.hitbox.is_hovered(window))
    }
}

#[derive(Clone)]
pub(super) struct ScrollbarLayout {
    pub hitbox: Hitbox,
    pub visible_range: Range<ScrollOffset>,
    pub text_unit_size: Pixels,
    pub thumb_bounds: Option<Bounds<Pixels>>,
    pub thumb_state: ScrollbarThumbState,
}

impl ScrollbarLayout {
    pub const BORDER_WIDTH: Pixels = px(1.0);
    const LINE_MARKER_HEIGHT: Pixels = px(2.0);
    const MIN_MARKER_HEIGHT: Pixels = px(5.0);
    const MIN_THUMB_SIZE: Pixels = px(25.0);

    fn new(
        scrollbar_track_hitbox: Hitbox,
        viewport_size: Pixels,
        scroll_range: Pixels,
        glyph_space: Pixels,
        content_offset: Pixels,
        scroll_position: ScrollOffset,
        show_thumb: bool,
        axis: ScrollbarAxis,
    ) -> Self {
        let track_bounds = scrollbar_track_hitbox.bounds;
        let track_length = track_bounds.size.along(axis) - content_offset;

        Self::new_with_hitbox_and_track_length(
            scrollbar_track_hitbox,
            track_length,
            viewport_size,
            scroll_range.into(),
            glyph_space,
            content_offset.into(),
            scroll_position,
            show_thumb,
            axis,
        )
    }

    pub fn for_minimap(
        minimap_track_hitbox: Hitbox,
        visible_lines: f64,
        total_editor_lines: f64,
        minimap_line_height: Pixels,
        scroll_position: ScrollOffset,
        minimap_scroll_top: ScrollOffset,
        show_thumb: bool,
    ) -> Self {
        let scroll_range = total_editor_lines * f64::from(minimap_line_height);
        let viewport_size = visible_lines * f64::from(minimap_line_height);
        let track_top_offset = -minimap_scroll_top * f64::from(minimap_line_height);

        Self::new_with_hitbox_and_track_length(
            minimap_track_hitbox,
            Pixels::from(scroll_range),
            Pixels::from(viewport_size),
            scroll_range,
            minimap_line_height,
            track_top_offset,
            scroll_position,
            show_thumb,
            ScrollbarAxis::Vertical,
        )
    }

    fn new_with_hitbox_and_track_length(
        scrollbar_track_hitbox: Hitbox,
        track_length: Pixels,
        viewport_size: Pixels,
        scroll_range: f64,
        glyph_space: Pixels,
        content_offset: ScrollOffset,
        scroll_position: ScrollOffset,
        show_thumb: bool,
        axis: ScrollbarAxis,
    ) -> Self {
        let text_units_per_page = viewport_size.to_f64() / glyph_space.to_f64();
        let visible_range = scroll_position..scroll_position + text_units_per_page;
        let total_text_units = scroll_range / glyph_space.to_f64();
        let thumb_percentage = text_units_per_page / total_text_units;
        let thumb_size = Pixels::from(ScrollOffset::from(track_length) * thumb_percentage)
            .max(ScrollbarLayout::MIN_THUMB_SIZE)
            .min(track_length);
        let text_unit_divisor = (total_text_units - text_units_per_page).max(0.);
        let content_larger_than_viewport = text_unit_divisor > 0.;
        let text_unit_size = if content_larger_than_viewport {
            Pixels::from(ScrollOffset::from(track_length - thumb_size) / text_unit_divisor)
        } else {
            glyph_space
        };

        let thumb_bounds = (show_thumb && content_larger_than_viewport).then(|| {
            Self::thumb_bounds(
                &scrollbar_track_hitbox,
                content_offset,
                visible_range.start,
                text_unit_size,
                thumb_size,
                axis,
            )
        });

        ScrollbarLayout {
            hitbox: scrollbar_track_hitbox,
            visible_range,
            text_unit_size,
            thumb_bounds,
            thumb_state: Default::default(),
        }
    }

    pub(super) fn with_thumb_state(self, thumb_state: Option<ScrollbarThumbState>) -> Self {
        if let Some(thumb_state) = thumb_state {
            Self {
                thumb_state,
                ..self
            }
        } else {
            self
        }
    }

    fn thumb_bounds(
        scrollbar_track: &Hitbox,
        content_offset: f64,
        visible_range_start: f64,
        text_unit_size: Pixels,
        thumb_size: Pixels,
        axis: ScrollbarAxis,
    ) -> Bounds<Pixels> {
        let thumb_origin = scrollbar_track.origin.apply_along(axis, |origin| {
            origin
                + Pixels::from(
                    content_offset + visible_range_start * ScrollOffset::from(text_unit_size),
                )
        });
        Bounds::new(
            thumb_origin,
            scrollbar_track.size.apply_along(axis, |_| thumb_size),
        )
    }

    pub fn thumb_hovered(&self, position: &gpui::Point<Pixels>) -> bool {
        self.thumb_bounds
            .is_some_and(|bounds| bounds.contains(position))
    }

    pub fn marker_quads_for_ranges(
        &self,
        row_ranges: impl IntoIterator<Item = ColoredRange<DisplayRow>>,
        column: Option<usize>,
    ) -> Vec<PaintQuad> {
        let (x_range, height_limit) = marker_x_range_and_height_limit(self, column);
        let row_to_y = |row: DisplayRow| row.as_f64() as f32 * self.text_unit_size;
        let mut pixel_ranges = row_ranges
            .into_iter()
            .map(|range| ColoredRange {
                start: row_to_y(range.start),
                end: row_to_y(range.end)
                    + self
                        .text_unit_size
                        .max(height_limit.min)
                        .min(height_limit.max),
                color: range.color,
            })
            .peekable();

        let mut quads = Vec::new();
        while let Some(mut pixel_range) = pixel_ranges.next() {
            while let Some(next_pixel_range) = pixel_ranges.peek() {
                if pixel_range.end >= next_pixel_range.start - px(1.0)
                    && pixel_range.color == next_pixel_range.color
                {
                    pixel_range.end = next_pixel_range.end.max(pixel_range.end);
                    pixel_ranges.next();
                } else {
                    break;
                }
            }

            quads.push(quad(
                Bounds::from_corners(
                    point(x_range.start, pixel_range.start),
                    point(x_range.end, pixel_range.end),
                ),
                Corners::default(),
                pixel_range.color,
                Edges::default(),
                Hsla::transparent_black(),
                BorderStyle::default(),
            ));
        }

        quads
    }
}

struct MinMax {
    min: Pixels,
    max: Pixels,
}

fn marker_x_range_and_height_limit(
    scrollbar: &ScrollbarLayout,
    column: Option<usize>,
) -> (Range<Pixels>, MinMax) {
    if let Some(column) = column {
        let column_width =
            ((scrollbar.hitbox.size.width - ScrollbarLayout::BORDER_WIDTH) / 3.0).floor();
        let start = ScrollbarLayout::BORDER_WIDTH + (column as f32 * column_width);
        (
            start..start + column_width,
            MinMax {
                min: ScrollbarLayout::MIN_MARKER_HEIGHT,
                max: px(f32::MAX),
            },
        )
    } else {
        (
            ScrollbarLayout::BORDER_WIDTH..scrollbar.hitbox.size.width,
            MinMax {
                min: ScrollbarLayout::LINE_MARKER_HEIGHT,
                max: ScrollbarLayout::LINE_MARKER_HEIGHT,
            },
        )
    }
}

pub(super) struct MinimapLayout {
    pub minimap: AnyElement,
    pub thumb_layout: ScrollbarLayout,
    pub minimap_scroll_top: ScrollOffset,
    pub minimap_line_height: Pixels,
    pub thumb_border_style: MinimapThumbBorder,
    pub max_scroll_top: ScrollOffset,
}

impl MinimapLayout {
    pub const MINIMAP_MIN_WIDTH_COLUMNS: f32 = 20.;
    pub const MINIMAP_WIDTH_PCT: f32 = 0.15;

    pub fn calculate_minimap_top_offset(
        document_lines: f64,
        visible_editor_lines: f64,
        visible_minimap_lines: f64,
        scroll_position: f64,
    ) -> ScrollOffset {
        let non_visible_document_lines = (document_lines - visible_editor_lines).max(0.);
        if non_visible_document_lines == 0. {
            0.
        } else {
            let scroll_percentage = (scroll_position / non_visible_document_lines).clamp(0., 1.);
            scroll_percentage * (document_lines - visible_minimap_lines).max(0.)
        }
    }
}

impl EditorElement {
    pub(super) fn layout_scrollbars(
        &self,
        snapshot: &EditorSnapshot,
        scrollbar_layout_information: &ScrollbarLayoutInformation,
        content_offset: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        non_visible_cursors: bool,
        right_margin: Pixels,
        editor_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<EditorScrollbars> {
        let show_scrollbars = self.editor.read(cx).show_scrollbars;
        if (!show_scrollbars.horizontal && !show_scrollbars.vertical)
            || self.style.scrollbar_width.is_zero()
        {
            return None;
        }

        // If a drag took place after we started dragging the scrollbar,
        // cancel the scrollbar drag.
        if cx.has_active_drag() {
            self.editor.update(cx, |editor, cx| {
                editor.scroll_manager.reset_scrollbar_state(cx)
            });
        }

        let editor_settings = EditorSettings::get_global(cx);
        let scrollbar_settings = editor_settings.scrollbar;
        let show_scrollbars = match scrollbar_settings.show {
            ShowScrollbar::Auto => {
                let editor = self.editor.read(cx);
                let is_singleton = editor.buffer_kind(cx) == ItemBufferKind::Singleton;
                // Git
                (is_singleton && scrollbar_settings.git_diff && snapshot.buffer_snapshot().has_diff_hunks())
                ||
                // Buffer Search Results
                (is_singleton && scrollbar_settings.search_results && editor.has_background_highlights(HighlightKey::BufferSearchHighlights))
                ||
                // Selected Text Occurrences
                (is_singleton && scrollbar_settings.selected_text && editor.has_background_highlights(HighlightKey::SelectedTextHighlight))
                ||
                // Selected Symbol Occurrences
                (is_singleton && scrollbar_settings.selected_symbol && (editor.has_background_highlights(HighlightKey::DocumentHighlightRead) || editor.has_background_highlights(HighlightKey::DocumentHighlightWrite)))
                ||
                // Diagnostics
                (is_singleton && scrollbar_settings.diagnostics != ScrollbarDiagnostics::None && snapshot.buffer_snapshot().has_diagnostics())
                ||
                // Cursors out of sight
                non_visible_cursors
                ||
                // Scrollmanager
                editor.scroll_manager.scrollbars_visible()
            }
            ShowScrollbar::System => self.editor.read(cx).scroll_manager.scrollbars_visible(),
            ShowScrollbar::Always => true,
            ShowScrollbar::Never => return None,
        };

        // The horizontal scrollbar is usually slightly offset to align nicely with
        // indent guides. However, this offset is not needed if indent guides are
        // disabled for the current editor.
        let content_offset = self
            .editor
            .read(cx)
            .show_indent_guides
            .is_none_or(|should_show| should_show)
            .then_some(content_offset)
            .unwrap_or_default();

        Some(EditorScrollbars::from_scrollbar_axes(
            ScrollbarAxes {
                horizontal: scrollbar_settings.axes.horizontal
                    && self.editor.read(cx).show_scrollbars.horizontal,
                vertical: scrollbar_settings.axes.vertical
                    && self.editor.read(cx).show_scrollbars.vertical,
            },
            scrollbar_layout_information,
            content_offset,
            scroll_position,
            self.style.scrollbar_width,
            right_margin,
            editor_width,
            show_scrollbars,
            self.editor.read(cx).scroll_manager.active_scrollbar_state(),
            window,
        ))
    }
}
