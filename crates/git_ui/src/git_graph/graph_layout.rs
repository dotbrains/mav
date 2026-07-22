use gpui::{Bounds, Hsla, PathBuilder, Pixels, Window, point, px};

pub(super) const COMMIT_CIRCLE_RADIUS: Pixels = px(3.5);
pub(super) const COMMIT_CIRCLE_STROKE_WIDTH: Pixels = px(1.5);
pub(super) const LANE_WIDTH: Pixels = px(16.0);
pub(super) const LEFT_PADDING: Pixels = px(12.0);
pub(super) const LINE_WIDTH: Pixels = px(1.5);

// Extra vertical breathing room added to the UI line height when computing
// the git graph's row height, so commit dots and lines have space around them.
pub(super) const ROW_VERTICAL_PADDING: Pixels = px(4.0);

pub(super) fn lane_center_x(bounds: Bounds<Pixels>, lane: f32) -> Pixels {
    bounds.origin.x + LEFT_PADDING + lane * LANE_WIDTH + LANE_WIDTH / 2.0
}

pub(super) fn to_row_center(
    to_row: usize,
    row_height: Pixels,
    scroll_offset: Pixels,
    bounds: Bounds<Pixels>,
) -> Pixels {
    bounds.origin.y + to_row as f32 * row_height + row_height / 2.0 - scroll_offset
}

pub(super) fn draw_commit_circle(
    center_x: Pixels,
    center_y: Pixels,
    color: Hsla,
    window: &mut Window,
) {
    let radius = COMMIT_CIRCLE_RADIUS;
    let mut builder = PathBuilder::fill();

    builder.move_to(point(center_x + radius, center_y));
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center_x - radius, center_y),
    );
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center_x + radius, center_y),
    );
    builder.close();

    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}
