#![cfg_attr(target_family = "wasm", no_main)]

use gpui::{
    App, Bounds, Context, Div, SharedString, Window, WindowBounds, WindowOptions, div, hsla,
    prelude::*, px, relative, rgb, size,
};
use gpui_platform::application;

#[path = "shadow/sections.rs"]
mod sections;

struct Shadow {}

impl Shadow {
    fn base() -> Div {
        div()
            .size_16()
            .bg(rgb(0xffffff))
            .rounded_full()
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.1))
    }

    fn square() -> Div {
        div()
            .size_16()
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.1))
    }

    fn rounded_small() -> Div {
        div()
            .size_16()
            .bg(rgb(0xffffff))
            .rounded(px(4.))
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.1))
    }

    fn rounded_medium() -> Div {
        div()
            .size_16()
            .bg(rgb(0xffffff))
            .rounded(px(8.))
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.1))
    }

    fn rounded_large() -> Div {
        div()
            .size_16()
            .bg(rgb(0xffffff))
            .rounded(px(12.))
            .border_1()
            .border_color(hsla(0.0, 0.0, 0.0, 0.1))
    }
}

fn example(label: impl Into<SharedString>, example: impl IntoElement) -> impl IntoElement {
    let label = label.into();

    div()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .w(relative(1. / 6.))
        .border_r_1()
        .border_color(hsla(0.0, 0.0, 0.0, 1.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_1()
                .py_12()
                .child(example),
        )
        .child(
            div()
                .w_full()
                .border_t_1()
                .border_color(hsla(0.0, 0.0, 0.0, 1.0))
                .p_1()
                .flex()
                .items_center()
                .child(label),
        )
}

impl Render for Shadow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("shadow-example")
            .overflow_y_scroll()
            .bg(rgb(0xffffff))
            .size_full()
            .text_xs()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .children(sections::shadow_rows()),
            )
    }
}

fn run_example() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Shadow {}),
        )
        .unwrap();

        cx.activate(true);
    });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_platform::web_init();
    run_example();
}
