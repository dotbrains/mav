use std::{env, sync::LazyLock};

use gpui::{
    App, Bounds, CursorStyle, Decorations, Global, HitboxBehavior, Hsla, IntoElement, MouseButton,
    Pixels, Point, ResizeEdge, Size, Stateful, Tiling, canvas, point, px, size, transparent_black,
};
use theme::{ActiveTheme as _, ClientDecorationsExt as _};
use ui::{Div, Window, prelude::*};

static MAV_WINDOW_SIZE: LazyLock<Option<Size<Pixels>>> = LazyLock::new(|| {
    env::var("MAV_WINDOW_SIZE")
        .ok()
        .as_deref()
        .and_then(parse_pixel_size_env_var)
});

static MAV_WINDOW_POSITION: LazyLock<Option<Point<Pixels>>> = LazyLock::new(|| {
    env::var("MAV_WINDOW_POSITION")
        .ok()
        .as_deref()
        .and_then(parse_pixel_position_env_var)
});

pub(crate) fn window_bounds_env_override() -> Option<Bounds<Pixels>> {
    MAV_WINDOW_POSITION
        .zip(*MAV_WINDOW_SIZE)
        .map(|(position, size)| Bounds {
            origin: position,
            size,
        })
}

fn parse_pixel_position_env_var(value: &str) -> Option<Point<Pixels>> {
    let mut parts = value.split(',');
    let x: usize = parts.next()?.parse().ok()?;
    let y: usize = parts.next()?.parse().ok()?;
    Some(point(px(x as f32), px(y as f32)))
}

fn parse_pixel_size_env_var(value: &str) -> Option<Size<Pixels>> {
    let mut parts = value.split(',');
    let width: usize = parts.next()?.parse().ok()?;
    let height: usize = parts.next()?.parse().ok()?;
    Some(size(px(width as f32), px(height as f32)))
}

/// Add client-side decorations (rounded corners, shadows, resize handling) when
/// appropriate.
///
/// The `border_radius_tiling` parameter allows overriding which corners get
/// rounded, independently of the actual window tiling state. This is used
/// specifically for the workspace switcher sidebar: when the sidebar is open,
/// we want square corners on the left (so the sidebar appears flush with the
/// window edge) but we still need the shadow padding for proper visual
/// appearance. Unlike actual window tiling, this only affects border radius -
/// not padding or shadows.
pub fn client_side_decorations(
    element: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
    border_radius_tiling: Tiling,
) -> Stateful<Div> {
    const BORDER_SIZE: Pixels = px(1.0);
    let decorations = window.window_decorations();
    let tiling = match decorations {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    };
    let corner_tiling = Tiling {
        top: tiling.top || border_radius_tiling.top,
        bottom: tiling.bottom || border_radius_tiling.bottom,
        left: tiling.left || border_radius_tiling.left,
        right: tiling.right || border_radius_tiling.right,
    };

    match decorations {
        Decorations::Client { .. } => window.set_client_inset(theme::CLIENT_SIDE_DECORATION_SHADOW),
        Decorations::Server => window.set_client_inset(px(0.0)),
    }

    struct GlobalResizeEdge(ResizeEdge);
    impl Global for GlobalResizeEdge {}

    div()
        .id("window-backdrop")
        .bg(transparent_black())
        .map(|div| match decorations {
            Decorations::Server => div,
            Decorations::Client { .. } => div
                .rounded_client_corners(corner_tiling)
                .when(!tiling.top, |div| {
                    div.pt(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.bottom, |div| {
                    div.pb(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.left, |div| {
                    div.pl(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.right, |div| {
                    div.pr(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .on_mouse_move(move |e, window, cx| {
                    let size = window.window_bounds().get_bounds().size;
                    let pos = e.position;

                    let new_edge =
                        resize_edge(pos, theme::CLIENT_SIDE_DECORATION_SHADOW, size, tiling);

                    let edge = cx.try_global::<GlobalResizeEdge>();
                    if new_edge != edge.map(|edge| edge.0) {
                        window
                            .window_handle()
                            .update(cx, |workspace, _, cx| {
                                cx.notify(workspace.entity_id());
                            })
                            .ok();
                    }
                })
                .on_mouse_down(MouseButton::Left, move |e, window, _| {
                    let size = window.window_bounds().get_bounds().size;
                    let pos = e.position;

                    let edge = match resize_edge(
                        pos,
                        theme::CLIENT_SIDE_DECORATION_SHADOW,
                        size,
                        tiling,
                    ) {
                        Some(value) => value,
                        None => return,
                    };

                    window.start_window_resize(edge);
                }),
        })
        .size_full()
        .child(
            div()
                .cursor(CursorStyle::Arrow)
                .map(|div| match decorations {
                    Decorations::Server => div,
                    Decorations::Client { .. } => div
                        .border_color(cx.theme().colors().border)
                        .rounded_client_corners(corner_tiling)
                        .when(!tiling.top, |div| div.border_t(BORDER_SIZE))
                        .when(!tiling.bottom, |div| div.border_b(BORDER_SIZE))
                        .when(!tiling.left, |div| div.border_l(BORDER_SIZE))
                        .when(!tiling.right, |div| div.border_r(BORDER_SIZE))
                        .when(!tiling.is_tiled(), |div| {
                            div.shadow(vec![
                                gpui::BoxShadow::new(
                                    px(0.),
                                    px(0.),
                                    Hsla {
                                        h: 0.,
                                        s: 0.,
                                        l: 0.,
                                        a: 0.4,
                                    },
                                )
                                .blur_radius(theme::CLIENT_SIDE_DECORATION_SHADOW / 2.),
                            ])
                        }),
                })
                .on_mouse_move(|_e, _, cx| {
                    cx.stop_propagation();
                })
                .size_full()
                .child(element),
        )
        .map(|div| match decorations {
            Decorations::Server => div,
            Decorations::Client { tiling, .. } => div.child(
                canvas(
                    |_bounds, window, _| {
                        window.insert_hitbox(
                            Bounds::new(
                                point(px(0.0), px(0.0)),
                                window.window_bounds().get_bounds().size,
                            ),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_bounds, hitbox, window, cx| {
                        let mouse = window.mouse_position();
                        let size = window.window_bounds().get_bounds().size;
                        let Some(edge) =
                            resize_edge(mouse, theme::CLIENT_SIDE_DECORATION_SHADOW, size, tiling)
                        else {
                            return;
                        };
                        cx.set_global(GlobalResizeEdge(edge));
                        window.set_cursor_style(
                            match edge {
                                ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                                ResizeEdge::Left | ResizeEdge::Right => {
                                    CursorStyle::ResizeLeftRight
                                }
                                ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                    CursorStyle::ResizeUpLeftDownRight
                                }
                                ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                    CursorStyle::ResizeUpRightDownLeft
                                }
                            },
                            &hitbox,
                        );
                    },
                )
                .size_full()
                .absolute(),
            ),
        })
}

fn resize_edge(
    pos: Point<Pixels>,
    shadow_size: Pixels,
    window_size: Size<Pixels>,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let bounds = Bounds::new(Point::default(), window_size).inset(shadow_size * 1.5);
    if bounds.contains(&pos) {
        return None;
    }

    let corner_size = size(shadow_size * 1.5, shadow_size * 1.5);
    let top_left_bounds = Bounds::new(Point::new(px(0.), px(0.)), corner_size);
    if !tiling.top && top_left_bounds.contains(&pos) {
        return Some(ResizeEdge::TopLeft);
    }

    let top_right_bounds = Bounds::new(
        Point::new(window_size.width - corner_size.width, px(0.)),
        corner_size,
    );
    if !tiling.top && top_right_bounds.contains(&pos) {
        return Some(ResizeEdge::TopRight);
    }

    let bottom_left_bounds = Bounds::new(
        Point::new(px(0.), window_size.height - corner_size.height),
        corner_size,
    );
    if !tiling.bottom && bottom_left_bounds.contains(&pos) {
        return Some(ResizeEdge::BottomLeft);
    }

    let bottom_right_bounds = Bounds::new(
        Point::new(
            window_size.width - corner_size.width,
            window_size.height - corner_size.height,
        ),
        corner_size,
    );
    if !tiling.bottom && bottom_right_bounds.contains(&pos) {
        return Some(ResizeEdge::BottomRight);
    }

    if !tiling.top && pos.y < shadow_size {
        Some(ResizeEdge::Top)
    } else if !tiling.bottom && pos.y > window_size.height - shadow_size {
        Some(ResizeEdge::Bottom)
    } else if !tiling.left && pos.x < shadow_size {
        Some(ResizeEdge::Left)
    } else if !tiling.right && pos.x > window_size.width - shadow_size {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}
