use super::*;
use crate::Tooltip;

impl<T: ButtonBuilder, const COLS: usize, const ROWS: usize> Component
    for ToggleButtonGroup<T, COLS, ROWS>
{
    fn name() -> &'static str {
        "ToggleButtonGroup"
    }

    fn scope() -> ComponentScope {
        ComponentScope::Input
    }

    fn sort_name() -> &'static str {
        "ButtonG"
    }

    fn description() -> &'static str {
        "A grouped set of toggle buttons arranged in rows and columns, \
        where each button represents a mutually exclusive option in a segmented control."
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> AnyElement {
        v_flex()
            .gap_6()
            .children(vec![example_group_with_title(
                "Transparent Variant",
                vec![
                    single_example(
                        "Single Row Group",
                        ToggleButtonGroup::single_row(
                            "single_row_test",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                        )
                        .selected_index(1)
                        .into_any_element(),
                    ),
                    single_example(
                        "Single Row Group with icons",
                        ToggleButtonGroup::single_row(
                            "single_row_test_icon",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(1)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                            [
                                ToggleButtonSimple::new("Fourth", |_, _, _| {}),
                                ToggleButtonSimple::new("Fifth", |_, _, _| {}),
                                ToggleButtonSimple::new("Sixth", |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group with Icons",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test_icons",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                            [
                                ToggleButtonWithIcon::new("Fourth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Fifth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Sixth", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .into_any_element(),
                    ),
                ],
            )])
            .children(vec![example_group_with_title(
                "Outlined Variant",
                vec![
                    single_example(
                        "Single Row Group",
                        ToggleButtonGroup::single_row(
                            "single_row_test_outline",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                        )
                        .selected_index(1)
                        .style(ToggleButtonGroupStyle::Outlined)
                        .into_any_element(),
                    ),
                    single_example(
                        "Single Row Group with icons",
                        ToggleButtonGroup::single_row(
                            "single_row_test_icon_outlined",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(1)
                        .style(ToggleButtonGroupStyle::Outlined)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                            [
                                ToggleButtonSimple::new("Fourth", |_, _, _| {}),
                                ToggleButtonSimple::new("Fifth", |_, _, _| {}),
                                ToggleButtonSimple::new("Sixth", |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .style(ToggleButtonGroupStyle::Outlined)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group with Icons",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                            [
                                ToggleButtonWithIcon::new("Fourth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Fifth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Sixth", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .style(ToggleButtonGroupStyle::Outlined)
                        .into_any_element(),
                    ),
                ],
            )])
            .children(vec![example_group_with_title(
                "Filled Variant",
                vec![
                    single_example(
                        "Single Row Group",
                        ToggleButtonGroup::single_row(
                            "single_row_test_outline",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                        )
                        .selected_index(2)
                        .style(ToggleButtonGroupStyle::Filled)
                        .into_any_element(),
                    ),
                    single_example(
                        "Single Row Group with icons",
                        ToggleButtonGroup::single_row(
                            "single_row_test_icon_outlined",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(1)
                        .style(ToggleButtonGroupStyle::Filled)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test",
                            [
                                ToggleButtonSimple::new("First", |_, _, _| {}),
                                ToggleButtonSimple::new("Second", |_, _, _| {}),
                                ToggleButtonSimple::new("Third", |_, _, _| {}),
                            ],
                            [
                                ToggleButtonSimple::new("Fourth", |_, _, _| {}),
                                ToggleButtonSimple::new("Fifth", |_, _, _| {}),
                                ToggleButtonSimple::new("Sixth", |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .width(rems_from_px(100.))
                        .style(ToggleButtonGroupStyle::Filled)
                        .into_any_element(),
                    ),
                    single_example(
                        "Multiple Row Group with Icons",
                        ToggleButtonGroup::two_rows(
                            "multiple_row_test",
                            [
                                ToggleButtonWithIcon::new("First", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Second", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Third", IconName::AiMav, |_, _, _| {}),
                            ],
                            [
                                ToggleButtonWithIcon::new("Fourth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Fifth", IconName::AiMav, |_, _, _| {}),
                                ToggleButtonWithIcon::new("Sixth", IconName::AiMav, |_, _, _| {}),
                            ],
                        )
                        .selected_index(3)
                        .width(rems_from_px(100.))
                        .style(ToggleButtonGroupStyle::Filled)
                        .into_any_element(),
                    ),
                ],
            )])
            .children(vec![single_example(
                "With Tooltips",
                ToggleButtonGroup::single_row(
                    "with_tooltips",
                    [
                        ToggleButtonSimple::new("First", |_, _, _| {})
                            .tooltip(Tooltip::text("This is a tooltip. Hello!")),
                        ToggleButtonSimple::new("Second", |_, _, _| {})
                            .tooltip(Tooltip::text("This is a tooltip. Hey?")),
                        ToggleButtonSimple::new("Third", |_, _, _| {})
                            .tooltip(Tooltip::text("This is a tooltip. Get out of here now!")),
                    ],
                )
                .selected_index(1)
                .into_any_element(),
            )])
            .into_any_element()
    }
}
