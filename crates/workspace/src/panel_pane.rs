use crate::{
    Panel,
    dock::PanelHandle,
    item::{Item, TabContentParams},
    pane::{Pane, PaneKind, render_toggle_zoom_button},
};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
};
use std::sync::Arc;
use ui::{Icon, IconButton, IconSize, Label, LabelCommon, TabBar, Tooltip, prelude::*};

pub const PROJECT_PANEL_KEYS: &[&str] = &[
    "ProjectPanel",
    "GitPanel",
    "OutlinePanel",
    "CollaborationPanel",
];

const AGENT_PANEL_KEY: &str = "agent_panel";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelPaneKind {
    Project,
    Agent,
}

impl PanelPaneKind {
    pub fn for_panel_key(panel_key: &str) -> Option<Self> {
        if PROJECT_PANEL_KEYS.contains(&panel_key) {
            Some(Self::Project)
        } else if panel_key == AGENT_PANEL_KEY {
            Some(Self::Agent)
        } else {
            None
        }
    }

    pub fn pane_kind(self) -> PaneKind {
        match self {
            Self::Project => PaneKind::Project,
            Self::Agent => PaneKind::Agent,
        }
    }
}

pub struct PanelItem {
    panel: Arc<dyn PanelHandle>,
}

impl PanelItem {
    pub fn new(panel: Arc<dyn PanelHandle>) -> Self {
        Self { panel }
    }

    pub fn panel(&self) -> Arc<dyn PanelHandle> {
        self.panel.clone()
    }

    pub fn panel_id(&self) -> gpui::EntityId {
        self.panel.panel_id()
    }

    pub fn panel_key(&self) -> &'static str {
        self.panel.panel_key()
    }

    pub fn is_panel<T: Panel>(&self) -> bool {
        self.panel.to_any().downcast::<T>().is_ok()
    }
}

impl Focusable for PanelItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.panel.panel_focus_handle(cx)
    }
}

impl EventEmitter<()> for PanelItem {}

impl Render for PanelItem {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.panel.to_any()
    }
}

impl Item for PanelItem {
    type Event = ();

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        self.panel.persistent_name().into()
    }

    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> gpui::AnyElement {
        h_flex()
            .gap_1()
            .when_some(self.tab_icon(window, cx), |this, icon| {
                this.child(icon.size(IconSize::XSmall))
            })
            .child(
                Label::new(self.tab_content_text(params.detail.unwrap_or_default(), cx))
                    .color(params.text_color()),
            )
            .into_any_element()
    }

    fn tab_icon(&self, window: &Window, cx: &App) -> Option<Icon> {
        self.panel.icon(window, cx).map(Icon::new)
    }

    fn activated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panel.set_active(true, window, cx);
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panel.set_active(false, window, cx);
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn include_in_nav_history() -> bool {
        false
    }
}

pub fn configure_project_pane(pane: &mut Pane, cx: &mut Context<Pane>) {
    pane.set_pane_kind(PaneKind::Project, cx);
    pane.set_close_pane_if_empty(false, cx);
    pane.set_should_display_tab_bar(|_, _| true);
    pane.set_render_tab_bar(cx, render_project_pane_header);
}

pub fn configure_agent_pane(pane: &mut Pane, cx: &mut Context<Pane>) {
    pane.set_pane_kind(PaneKind::Agent, cx);
    pane.set_close_pane_if_empty(false, cx);
    pane.set_should_display_tab_bar(|_, _| false);
}

fn render_project_pane_header(
    pane: &mut Pane,
    window: &mut Window,
    cx: &mut Context<Pane>,
) -> gpui::AnyElement {
    let active_item_id = pane.active_item().map(|item| item.item_id());
    let mut buttons = pane
        .items()
        .enumerate()
        .filter_map(|(ix, item)| {
            let panel_item = item.downcast::<PanelItem>()?;
            let panel = panel_item.read(cx).panel();
            let panel_order = PROJECT_PANEL_KEYS
                .iter()
                .position(|panel_key| *panel_key == panel.panel_key())
                .unwrap_or(PROJECT_PANEL_KEYS.len());
            let is_active = active_item_id == Some(panel_item.entity_id());
            if !is_active && !panel.button_visible(cx) {
                return None;
            }

            let icon = panel.icon(window, cx)?;
            let tooltip = panel
                .icon_tooltip(window, cx)
                .unwrap_or_else(|| panel.persistent_name());

            Some((panel_order, ix, panel.panel_id(), icon, tooltip, is_active))
        })
        .collect::<Vec<_>>();
    buttons.sort_by_key(|(panel_order, _, _, _, _, _)| *panel_order);

    let buttons = buttons
        .into_iter()
        .map(|(_, ix, panel_id, icon, tooltip, is_active)| {
            IconButton::new(("project-pane-panel", panel_id), icon)
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted)
                .toggle_state(is_active)
                .tooltip(Tooltip::text(tooltip))
                .on_click(cx.listener(move |pane, _, window, cx| {
                    pane.activate_item(ix, true, true, window, cx);
                }))
        })
        .collect::<Vec<_>>();

    TabBar::new("project_pane_header")
        .child(h_flex().h_full().px_1p5().gap_1().children(buttons))
        .when(pane.is_zoomed(), |tab_bar| {
            tab_bar.end_child(render_toggle_zoom_button(pane, cx))
        })
        .into_any_element()
}
