use gpui::{App, Entity, FocusHandle};

use crate::{Dock, Pane, PanelPaneKind};

#[derive(Clone)]
pub(super) enum ActivateInDirectionTarget {
    Pane(Entity<Pane>),
    Dock(Entity<Dock>),
    Sidebar(FocusHandle),
}

pub(super) fn dock_has_focus_target(dock: &Entity<Dock>, cx: &App) -> bool {
    let dock = dock.read(cx);
    if !dock.is_open() {
        return false;
    }

    dock.active_panel()
        .is_some_and(|panel| PanelPaneKind::for_panel_key(panel.panel_key()).is_none())
}
