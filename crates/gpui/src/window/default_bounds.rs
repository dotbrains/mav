use super::*;

pub(super) fn default_bounds(display_id: Option<DisplayId>, cx: &mut App) -> WindowBounds {
    // TODO, BUG: if you open a window with the currently active window
    // on the stack, this will erroneously fallback to `None`
    //
    // TODO these should be the initial window bounds not considering maximized/fullscreen
    let active_window_bounds = cx
        .active_window()
        .and_then(|w| w.update(cx, |_, window, _| window.window_bounds()).ok());

    const CASCADE_OFFSET: f32 = 25.0;

    let display = display_id
        .map(|id| cx.find_display(id))
        .unwrap_or_else(|| cx.primary_display());

    let default_placement = || Bounds::new(point(px(0.), px(0.)), DEFAULT_WINDOW_SIZE);

    // Use visible_bounds to exclude taskbar/dock areas
    let display_bounds = display
        .as_ref()
        .map(|d| d.visible_bounds())
        .unwrap_or_else(default_placement);

    let (
        Bounds {
            origin: base_origin,
            size: base_size,
        },
        window_bounds_ctor,
    ): (_, fn(Bounds<Pixels>) -> WindowBounds) = match active_window_bounds {
        Some(bounds) => match bounds {
            WindowBounds::Windowed(bounds) => (bounds, WindowBounds::Windowed),
            WindowBounds::Maximized(bounds) => (bounds, WindowBounds::Maximized),
            WindowBounds::Fullscreen(bounds) => (bounds, WindowBounds::Fullscreen),
        },
        None => (
            display
                .as_ref()
                .map(|d| d.default_bounds())
                .unwrap_or_else(default_placement),
            WindowBounds::Windowed,
        ),
    };

    let cascade_offset = point(px(CASCADE_OFFSET), px(CASCADE_OFFSET));
    let proposed_origin = base_origin + cascade_offset;
    let proposed_bounds = Bounds::new(proposed_origin, base_size);

    let display_right = display_bounds.origin.x + display_bounds.size.width;
    let display_bottom = display_bounds.origin.y + display_bounds.size.height;
    let window_right = proposed_bounds.origin.x + proposed_bounds.size.width;
    let window_bottom = proposed_bounds.origin.y + proposed_bounds.size.height;

    let fits_horizontally = window_right <= display_right;
    let fits_vertically = window_bottom <= display_bottom;

    let final_origin = match (fits_horizontally, fits_vertically) {
        (true, true) => proposed_origin,
        (false, true) => point(display_bounds.origin.x, base_origin.y),
        (true, false) => point(base_origin.x, display_bounds.origin.y),
        (false, false) => display_bounds.origin,
    };
    window_bounds_ctor(Bounds::new(final_origin, base_size))
}
