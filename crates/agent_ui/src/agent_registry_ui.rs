use std::ops::Range;

use client::mav_urls;
use collections::HashMap;
use editor::{Editor, EditorElement, EditorStyle};
use fs::Fs;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Focusable, KeyContext, ParentElement, Render,
    RenderOnce, SharedString, Styled, TextStyle, UniformListScrollHandle, Window, point,
    uniform_list,
};
use project::agent_server_store::{AllAgentServersSettings, CustomAgentServerSettings};
use project::{AgentRegistryStore, RegistryAgent};
use settings::{Settings, SettingsStore, update_settings_file};
use theme_settings::ThemeSettings;
use ui::{
    ButtonStyle, ScrollableHandle, ToggleButtonGroup, ToggleButtonGroupSize,
    ToggleButtonGroupStyle, ToggleButtonSimple, Tooltip, WithScrollbar, prelude::*,
};
use workspace::{
    Workspace,
    item::{Item, ItemEvent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryFilter {
    All,
    Installed,
    NotInstalled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegistryInstallStatus {
    NotInstalled,
    InstalledRegistry,
    InstalledCustom,
}

#[derive(IntoElement)]
struct AgentRegistryCard {
    children: Vec<AnyElement>,
}

impl AgentRegistryCard {
    fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

impl ParentElement for AgentRegistryCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for AgentRegistryCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div().w_full().child(
            v_flex()
                .p_3()
                .mt_4()
                .w_full()
                .min_h(rems_from_px(86.))
                .gap_2()
                .bg(cx.theme().colors().elevated_surface_background.opacity(0.5))
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .rounded_md()
                .children(self.children),
        )
    }
}

pub struct AgentRegistryPage {
    registry_store: Entity<AgentRegistryStore>,
    list: UniformListScrollHandle,
    registry_agents: Vec<RegistryAgent>,
    filtered_registry_indices: Vec<usize>,
    installed_statuses: HashMap<String, RegistryInstallStatus>,
    query_editor: Entity<Editor>,
    filter: RegistryFilter,
    _subscriptions: Vec<gpui::Subscription>,
}

impl AgentRegistryPage {
    pub fn new(
        _workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let registry_store = AgentRegistryStore::global(cx);
            let query_editor = cx.new(|cx| {
                let mut input = Editor::single_line(window, cx);
                input.set_placeholder_text("Search agents...", window, cx);
                input
            });
            cx.subscribe(&query_editor, Self::on_query_change).detach();

            let mut subscriptions = Vec::new();
            subscriptions.push(cx.observe(&registry_store, |this, _, cx| {
                this.reload_registry_agents(cx);
            }));
            subscriptions.push(cx.observe_global::<SettingsStore>(|this, cx| {
                this.filter_registry_agents(cx);
            }));

            let mut this = Self {
                registry_store,
                list: UniformListScrollHandle::new(),
                registry_agents: Vec::new(),
                filtered_registry_indices: Vec::new(),
                installed_statuses: HashMap::default(),
                query_editor,
                filter: RegistryFilter::All,
                _subscriptions: subscriptions,
            };

            this.reload_registry_agents(cx);
            this.registry_store
                .update(cx, |store, cx| store.refresh(cx));

            this
        })
    }

    fn reload_registry_agents(&mut self, cx: &mut Context<Self>) {
        self.registry_agents = self.registry_store.read(cx).agents().to_vec();
        self.registry_agents.sort_by(|left, right| {
            left.name()
                .as_ref()
                .to_lowercase()
                .cmp(&right.name().as_ref().to_lowercase())
                .then_with(|| {
                    left.id()
                        .as_ref()
                        .to_lowercase()
                        .cmp(&right.id().as_ref().to_lowercase())
                })
        });
        self.filter_registry_agents(cx);
    }

    fn refresh_installed_statuses(&mut self, cx: &mut Context<Self>) {
        let settings = cx
            .global::<SettingsStore>()
            .get::<AllAgentServersSettings>(None);
        self.installed_statuses.clear();
        for (id, settings) in settings.iter() {
            let status = match settings {
                CustomAgentServerSettings::Registry { .. } => {
                    RegistryInstallStatus::InstalledRegistry
                }
                CustomAgentServerSettings::Custom { .. } => RegistryInstallStatus::InstalledCustom,
            };
            self.installed_statuses.insert(id.clone(), status);
        }
    }

    fn install_status(&self, id: &str) -> RegistryInstallStatus {
        self.installed_statuses
            .get(id)
            .copied()
            .unwrap_or(RegistryInstallStatus::NotInstalled)
    }

    fn search_query(&self, cx: &mut App) -> Option<String> {
        let search = self.query_editor.read(cx).text(cx);
        if search.trim().is_empty() {
            None
        } else {
            Some(search)
        }
    }

    fn filter_registry_agents(&mut self, cx: &mut Context<Self>) {
        self.refresh_installed_statuses(cx);
        let search = self.search_query(cx).map(|search| search.to_lowercase());
        let filter = self.filter;
        let installed_statuses = self.installed_statuses.clone();

        let filtered_indices = self
            .registry_agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                let matches_search = search.as_ref().is_none_or(|query| {
                    let query = query.as_str();
                    agent.id().as_ref().to_lowercase().contains(query)
                        || agent.name().as_ref().to_lowercase().contains(query)
                        || agent.description().as_ref().to_lowercase().contains(query)
                });

                let install_status = installed_statuses
                    .get(agent.id().as_ref())
                    .copied()
                    .unwrap_or(RegistryInstallStatus::NotInstalled);
                let matches_filter = match filter {
                    RegistryFilter::All => true,
                    RegistryFilter::Installed => {
                        install_status != RegistryInstallStatus::NotInstalled
                    }
                    RegistryFilter::NotInstalled => {
                        install_status == RegistryInstallStatus::NotInstalled
                    }
                };

                matches_search && matches_filter
            })
            .map(|(index, _)| index)
            .collect();

        self.filtered_registry_indices = filtered_indices;

        cx.notify();
    }

    fn scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.list.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn on_query_change(
        &mut self,
        _: Entity<Editor>,
        event: &editor::EditorEvent,
        cx: &mut Context<Self>,
    ) {
        if let editor::EditorEvent::Edited { .. } = event {
            self.filter_registry_agents(cx);
            self.scroll_to_top(cx);
        }
    }
}

mod render;

impl EventEmitter<ItemEvent> for AgentRegistryPage {}

impl Focusable for AgentRegistryPage {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.query_editor.read(cx).focus_handle(cx)
    }
}

impl Item for AgentRegistryPage {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "ACP Registry".into()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("ACP Registry Page Opened")
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(workspace::item::ItemEvent)) {
        f(*event)
    }
}
