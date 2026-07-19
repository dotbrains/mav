use component::{example_group_with_title, single_example};

use super::{ListItem, ListItemSpacing};
use crate::prelude::*;

pub(super) fn preview() -> AnyElement {
    v_flex()
        .gap_6()
        .children(vec![
            example_group_with_title(
                "Basic List Items",
                vec![
                    single_example(
                        "Simple",
                        ListItem::new("simple")
                            .child(Label::new("Simple list item"))
                            .into_any_element(),
                    ),
                    single_example(
                        "With Icon",
                        ListItem::new("with_icon")
                            .start_slot(Icon::new(IconName::File))
                            .child(Label::new("List item with icon"))
                            .into_any_element(),
                    ),
                    single_example(
                        "Selected",
                        ListItem::new("selected")
                            .toggle_state(true)
                            .start_slot(Icon::new(IconName::Check))
                            .child(Label::new("Selected item"))
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "List Item Spacing",
                vec![
                    single_example(
                        "Dense",
                        ListItem::new("dense")
                            .spacing(ListItemSpacing::Dense)
                            .child(Label::new("Dense spacing"))
                            .into_any_element(),
                    ),
                    single_example(
                        "Extra Dense",
                        ListItem::new("extra_dense")
                            .spacing(ListItemSpacing::ExtraDense)
                            .child(Label::new("Extra dense spacing"))
                            .into_any_element(),
                    ),
                    single_example(
                        "Sparse",
                        ListItem::new("sparse")
                            .spacing(ListItemSpacing::Sparse)
                            .child(Label::new("Sparse spacing"))
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "With Slots",
                vec![
                    single_example(
                        "End Slot",
                        ListItem::new("end_slot")
                            .child(Label::new("Item with end slot"))
                            .end_slot(Icon::new(IconName::ChevronRight))
                            .into_any_element(),
                    ),
                    single_example(
                        "With Toggle",
                        ListItem::new("with_toggle")
                            .toggle(Some(true))
                            .child(Label::new("Expandable item"))
                            .into_any_element(),
                    ),
                ],
            ),
            example_group_with_title(
                "States",
                vec![
                    single_example(
                        "Disabled",
                        ListItem::new("disabled")
                            .disabled(true)
                            .child(Label::new("Disabled item"))
                            .into_any_element(),
                    ),
                    single_example(
                        "Non-selectable",
                        ListItem::new("non_selectable")
                            .selectable(false)
                            .child(Label::new("Non-selectable item"))
                            .into_any_element(),
                    ),
                ],
            ),
        ])
        .into_any_element()
}
