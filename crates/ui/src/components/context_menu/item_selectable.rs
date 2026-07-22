use super::*;

impl ContextMenuItem {
    pub(crate) fn is_selectable(&self) -> bool {
        match self {
            ContextMenuItem::Header(_)
            | ContextMenuItem::HeaderWithLink(_, _, _)
            | ContextMenuItem::Separator
            | ContextMenuItem::Label { .. } => false,
            ContextMenuItem::Entry(ContextMenuEntry { disabled, .. }) => !disabled,
            ContextMenuItem::CustomEntry { selectable, .. } => *selectable,
            ContextMenuItem::Submenu { .. } => true,
        }
    }
}
