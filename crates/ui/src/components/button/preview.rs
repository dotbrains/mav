use gpui::AnyElement;

use crate::component_prelude::*;
use crate::{
    Button, ButtonCommon, ButtonStyle, Color, Disableable, Icon, IconName, TintColor, Toggleable,
    prelude::*,
};

pub(super) fn preview() -> AnyElement {
    v_flex()
        .gap_6()
        .children(vec![
            example_group_with_title(
                "Button Styles",
                vec![
                    single_example(
                        "Default",
                        Button::new("default", "Default").into_any_element(),
                    ),
                    single_example(
                        "Filled",
                        Button::new("filled", "Filled")
                            .style(ButtonStyle::Filled)
                            .into_any_element(),
                    ),
                    single_example(
                        "Subtle",
                        Button::new("outline", "Subtle")
                            .style(ButtonStyle::Subtle)
                            .into_any_element(),
                    ),
                    single_example(
                        "Tinted",
                        Button::new("tinted_accent_style", "Accent")
                            .style(ButtonStyle::Tinted(TintColor::Accent))
                            .into_any_element(),
                    ),
                    single_example(
                        "Transparent",
                        Button::new("transparent", "Transparent")
                            .style(ButtonStyle::Transparent)
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "Tint Styles",
                vec![
                    single_example(
                        "Accent",
                        Button::new("tinted_accent", "Accent")
                            .style(ButtonStyle::Tinted(TintColor::Accent))
                            .into_any_element(),
                    ),
                    single_example(
                        "Error",
                        Button::new("tinted_negative", "Error")
                            .style(ButtonStyle::Tinted(TintColor::Error))
                            .into_any_element(),
                    ),
                    single_example(
                        "Warning",
                        Button::new("tinted_warning", "Warning")
                            .style(ButtonStyle::Tinted(TintColor::Warning))
                            .into_any_element(),
                    ),
                    single_example(
                        "Success",
                        Button::new("tinted_positive", "Success")
                            .style(ButtonStyle::Tinted(TintColor::Success))
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "Special States",
                vec![
                    single_example(
                        "Default",
                        Button::new("default_state", "Default").into_any_element(),
                    ),
                    single_example(
                        "Disabled",
                        Button::new("disabled", "Disabled")
                            .disabled(true)
                            .into_any_element(),
                    ),
                    single_example(
                        "Selected",
                        Button::new("selected", "Selected")
                            .toggle_state(true)
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "Buttons with Icons",
                vec![
                    single_example(
                        "Start Icon",
                        Button::new("icon_start", "Start Icon")
                            .start_icon(Icon::new(IconName::Check))
                            .into_any_element(),
                    ),
                    single_example(
                        "End Icon",
                        Button::new("icon_end", "End Icon")
                            .end_icon(Icon::new(IconName::Check))
                            .into_any_element(),
                    ),
                    single_example(
                        "Both Icons",
                        Button::new("both_icons", "Both Icons")
                            .start_icon(Icon::new(IconName::Check))
                            .end_icon(Icon::new(IconName::ChevronDown))
                            .into_any_element(),
                    ),
                    single_example(
                        "Icon Color",
                        Button::new("icon_color", "Icon Color")
                            .start_icon(Icon::new(IconName::Check).color(Color::Accent))
                            .into_any_element(),
                    ),
                ],
            ),
        ])
        .into_any_element()
}
