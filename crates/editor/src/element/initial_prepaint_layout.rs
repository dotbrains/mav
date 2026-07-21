use super::*;

pub(super) struct InitialPrepaintLayout {
    pub(super) snapshot: EditorSnapshot,
    pub(super) font_size: Pixels,
    pub(super) line_height: Pixels,
    pub(super) em_width: Pixels,
    pub(super) em_advance: Pixels,
    pub(super) em_layout_width: Pixels,
    pub(super) glyph_grid_cell: Size<Pixels>,
    pub(super) gutter_dimensions: GutterDimensions,
    pub(super) vertical_scrollbar_width: Pixels,
    pub(super) minimap_width: Pixels,
    pub(super) right_margin: Pixels,
    pub(super) editor_width: Pixels,
    pub(super) editor_margins: EditorMargins,
    pub(super) hitbox: Hitbox,
    pub(super) gutter_hitbox: Hitbox,
    pub(super) text_hitbox: Hitbox,
    pub(super) content_offset: gpui::Point<Pixels>,
    pub(super) content_origin: gpui::Point<Pixels>,
    pub(super) height_in_lines: f64,
    pub(super) max_scroll_top: f64,
    pub(super) scroll_beyond_last_line: ScrollBeyondLastLine,
    pub(super) autoscroll_request: Option<(Autoscroll, bool)>,
    pub(super) autoscroll_containing_element: bool,
    pub(super) needs_horizontal_autoscroll: NeedsHorizontalAutoscroll,
    pub(super) scroll_position: gpui::Point<ScrollOffset>,
    pub(super) visible_rows: layout_data::VisibleRows,
}

impl EditorElement {
    pub(super) fn layout_initial_prepaint(
        &self,
        bounds: Bounds<Pixels>,
        snapshot: EditorSnapshot,
        style: &EditorStyle,
        rem_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> InitialPrepaintLayout {
        let layout_data::EditorMetrics {
            font_size,
            line_height,
            em_width,
            em_advance,
            em_layout_width,
            glyph_grid_cell,
            gutter_dimensions,
            text_width,
            vertical_scrollbar_width,
            minimap_width,
            right_margin,
            editor_width,
            editor_margins,
        } = self.layout_metrics(bounds, &snapshot, style, rem_size, window, cx);

        let mut snapshot = self.update_snapshot_layout(
            bounds,
            snapshot,
            gutter_dimensions,
            line_height,
            editor_width,
            em_advance,
            em_layout_width,
            window,
            cx,
        );

        let surface = Self::layout_surface(bounds, text_width, &editor_margins, window);
        let hitbox = surface.hitbox;
        let gutter_hitbox = surface.gutter_hitbox;
        let text_hitbox = surface.text_hitbox;
        let content_offset = surface.content_offset;
        let content_origin = surface.content_origin;

        let height_in_lines = f64::from(bounds.size.height / line_height);
        let max_scroll_row = snapshot.max_point().row().as_f64();
        let scroll_beyond_last_line = self.editor.read(cx).scroll_beyond_last_line(cx);
        let max_scroll_top = match scroll_beyond_last_line {
            ScrollBeyondLastLine::OnePage => max_scroll_row,
            ScrollBeyondLastLine::Off => (max_scroll_row - height_in_lines + 1.).max(0.),
            ScrollBeyondLastLine::VerticalScrollMargin => {
                let settings = EditorSettings::get_global(cx);
                (max_scroll_row - height_in_lines + 1. + settings.vertical_scroll_margin).max(0.)
            }
        };

        let layout_data::VerticalAutoscroll {
            autoscroll_request,
            autoscroll_containing_element,
            needs_horizontal_autoscroll,
        } = self.layout_vertical_autoscroll(
            bounds,
            line_height,
            max_scroll_top,
            &mut snapshot,
            window,
            cx,
        );

        let mut scroll_position = snapshot.scroll_position();
        if !line_height.is_zero() {
            scroll_position.y = window.pixel_snap_f64(scroll_position.y * f64::from(line_height))
                / f64::from(line_height);
        }
        let visible_rows =
            Self::visible_rows(bounds, line_height, scroll_position, &snapshot, window);

        InitialPrepaintLayout {
            snapshot,
            font_size,
            line_height,
            em_width,
            em_advance,
            em_layout_width,
            glyph_grid_cell,
            gutter_dimensions,
            vertical_scrollbar_width,
            minimap_width,
            right_margin,
            editor_width,
            editor_margins,
            hitbox,
            gutter_hitbox,
            text_hitbox,
            content_offset,
            content_origin,
            height_in_lines,
            max_scroll_top,
            scroll_beyond_last_line,
            autoscroll_request,
            autoscroll_containing_element,
            needs_horizontal_autoscroll,
            scroll_position,
            visible_rows,
        }
    }
}
