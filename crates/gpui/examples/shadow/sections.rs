use super::*;
use gpui::BoxShadow;

pub(super) fn shadow_rows() -> Vec<Div> {
    vec![
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .flex_row()
            .children(vec![
                example(
                    "Square",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded 4",
                    Shadow::rounded_small().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded 8",
                    Shadow::rounded_medium().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded 16",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Circle",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
            ]),
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .w_full()
            .children(vec![
                example("None", Shadow::base()),
                // 2Xsmall shadow
                example("2X Small", Shadow::base().shadow_2xs()),
                // Xsmall shadow
                example("Extra Small", Shadow::base().shadow_xs()),
                // Small shadow
                example("Small", Shadow::base().shadow_sm()),
                // Medium shadow
                example("Medium", Shadow::base().shadow_md()),
                // Large shadow
                example("Large", Shadow::base().shadow_lg()),
                example("Extra Large", Shadow::base().shadow_xl()),
                example("2X Large", Shadow::base().shadow_2xl()),
            ]),
        // Horizontal list of increasing blur radii
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Blur 0",
                    Shadow::base().shadow(vec![BoxShadow::new(
                        px(0.),
                        px(8.),
                        hsla(0.0, 0.0, 0.0, 0.3),
                    )]),
                ),
                example(
                    "Blur 2",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(2.)),
                    ]),
                ),
                example(
                    "Blur 4",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(4.)),
                    ]),
                ),
                example(
                    "Blur 8",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Blur 16",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(16.)),
                    ]),
                ),
            ]),
        // Horizontal list of increasing spread radii
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Spread 0",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Spread 2",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                    ]),
                ),
                example(
                    "Spread 4",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(4.)),
                    ]),
                ),
                example(
                    "Spread 8",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(8.)),
                    ]),
                ),
                example(
                    "Spread 16",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(16.)),
                    ]),
                ),
            ]),
        // Square spread examples
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Square Spread 0",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Square Spread 8",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(8.)),
                    ]),
                ),
                example(
                    "Square Spread 16",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(16.)),
                    ]),
                ),
            ]),
        // Rounded large spread examples
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Rounded Large Spread 0",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded Large Spread 8",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded Large Spread 16",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(16.)),
                    ]),
                ),
            ]),
        // Directional shadows
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Left",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(-8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Right",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Top",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(-8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Bottom",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
            ]),
        // Square directional shadows
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Square Left",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(-8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Square Right",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Square Top",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(-8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Square Bottom",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
            ]),
        // Rounded large directional shadows
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Rounded Large Left",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(-8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded Large Right",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(8.), px(0.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded Large Top",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(-8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
                example(
                    "Rounded Large Bottom",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.5, 0.5, 0.3))
                            .blur_radius(px(8.)),
                    ]),
                ),
            ]),
        // Multiple shadows for different shapes
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .children(vec![
                example(
                    "Circle Multiple",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(-12.), hsla(0.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(12.), px(0.), hsla(60.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(0.), px(12.), hsla(120.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(-12.), px(0.), hsla(240.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                    ]),
                ),
                example(
                    "Square Multiple",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(-12.), hsla(0.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(12.), px(0.), hsla(60.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(0.), px(12.), hsla(120.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(-12.), px(0.), hsla(240.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                    ]),
                ),
                example(
                    "Rounded Large Multiple",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(-12.), hsla(0.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(12.), px(0.), hsla(60.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(0.), px(12.), hsla(120.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                        BoxShadow::new(px(-12.), px(0.), hsla(240.0 / 360., 1.0, 0.5, 0.3))
                            .blur_radius(px(8.))
                            .spread_radius(px(2.)),
                    ]),
                ),
            ]),
        // Inset shadows (CSS `box-shadow: inset ...`).
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .w_full()
            .children(vec![
                example(
                    "Inset basic",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(0.), hsla(0.0, 0.0, 0.0, 0.5))
                            .blur_radius(px(12.))
                            .inset(),
                    ]),
                ),
                example(
                    "Inset offset",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(6.), px(6.), hsla(0.0, 0.0, 0.0, 0.5))
                            .blur_radius(px(8.))
                            .inset(),
                    ]),
                ),
                example(
                    "Inset spread",
                    Shadow::base().shadow(vec![
                        BoxShadow::new(px(0.), px(0.), hsla(0.0, 0.0, 0.0, 0.5))
                            .blur_radius(px(4.))
                            .spread_radius(px(8.))
                            .inset(),
                    ]),
                ),
                example(
                    "Inset rounded",
                    Shadow::rounded_large().shadow(vec![
                        BoxShadow::new(px(0.), px(4.), hsla(0.0, 0.0, 0.0, 0.5))
                            .blur_radius(px(10.))
                            .spread_radius(px(2.))
                            .inset(),
                    ]),
                ),
                example(
                    "Inset sharp",
                    Shadow::square().shadow(vec![
                        BoxShadow::new(px(0.), px(0.), hsla(0.0, 0.0, 0.0, 0.6))
                            .spread_radius(px(6.))
                            .inset(),
                    ]),
                ),
            ]),
        // Combined: drop + inset shadows on the same element.
        div()
            .border_b_1()
            .border_color(hsla(0.0, 0.0, 0.0, 1.0))
            .flex()
            .w_full()
            .children(vec![example(
                "Drop + Inset",
                Shadow::rounded_medium().shadow(vec![
                    BoxShadow::new(px(0.), px(8.), hsla(0.0, 0.0, 0.0, 0.25)).blur_radius(px(12.)),
                    BoxShadow::new(px(0.), px(2.), hsla(0.0, 0.0, 0.0, 0.4))
                        .blur_radius(px(4.))
                        .inset(),
                ]),
            )]),
    ]
}
