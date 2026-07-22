use super::*;

struct EntryColors {
    default: Hsla,
    hover: Hsla,
    marked_active: Hsla,
}

pub(super) fn get_entry_color(cx: &Context<VariableList>) -> EntryColors {
    let colors = cx.theme().colors();

    EntryColors {
        default: colors.panel_background,
        hover: colors.ghost_element_hover,
        marked_active: colors.ghost_element_selected,
    }
}
