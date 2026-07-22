use crate::{
    CollaboratorId, DelayedDebouncedEditAction, FollowableViewRegistry, ItemNavHistory,
    SerializableItemRegistry, ToolbarItemLocation, ViewId, Workspace, WorkspaceId,
    invalid_item_view::InvalidItemView,
    pane::{self, Pane, SaveIntent},
    persistence::model::ItemId,
    searchable::SearchableItemHandle,
    workspace_settings::{AutosaveSetting, WorkspaceSettings},
};
use anyhow::Result;
use client::{Client, proto};
use futures::channel::mpsc;
use gpui::{
    Action, AnyElement, AnyEntity, AnyView, App, AppContext, Context, Entity, EntityId,
    EventEmitter, FocusHandle, Focusable, Font, ParentElement, Pixels, Point, Render, SharedString,
    Styled, Task, TaskExt, WeakEntity, Window,
};
use language::Capability;
pub use language::HighlightedText;
use project::{Project, ProjectEntryId, ProjectPath};
pub use settings::{
    ActivateOnClose, ClosePosition, RegisterSetting, Settings, SettingsLocation, ShowCloseButton,
    ShowDiagnostics,
};
use smallvec::SmallVec;
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use ui::{Color, FluentBuilder, Icon, IconName, IntoElement, Label, LabelCommon, div, h_flex};
use util::ResultExt;

pub const LEADER_UPDATE_THROTTLE: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug)]
pub struct SaveOptions {
    pub format: bool,
    pub force_format: bool,
    pub autosave: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            format: true,
            force_format: false,
            autosave: false,
        }
    }
}

#[derive(RegisterSetting)]
pub struct ItemSettings {
    pub git_status: bool,
    pub close_position: ClosePosition,
    pub activate_on_close: ActivateOnClose,
    pub file_icons: bool,
    pub show_diagnostics: ShowDiagnostics,
    pub show_close_button: ShowCloseButton,
}

#[derive(RegisterSetting)]
pub struct PreviewTabsSettings {
    pub enabled: bool,
    pub enable_preview_from_project_panel: bool,
    pub enable_preview_from_file_finder: bool,
    pub enable_preview_from_multibuffer: bool,
    pub enable_preview_multibuffer_from_code_navigation: bool,
    pub enable_preview_file_from_code_navigation: bool,
    pub enable_keep_preview_on_code_navigation: bool,
}

impl Settings for ItemSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let tabs = content.tabs.as_ref().unwrap();
        Self {
            git_status: tabs.git_status.unwrap()
                && content
                    .git
                    .as_ref()
                    .unwrap()
                    .enabled
                    .unwrap()
                    .is_git_status_enabled(),
            close_position: tabs.close_position.unwrap(),
            activate_on_close: tabs.activate_on_close.unwrap(),
            file_icons: tabs.file_icons.unwrap(),
            show_diagnostics: tabs.show_diagnostics.unwrap(),
            show_close_button: tabs.show_close_button.unwrap(),
        }
    }
}

impl Settings for PreviewTabsSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let preview_tabs = content.preview_tabs.as_ref().unwrap();
        Self {
            enabled: preview_tabs.enabled.unwrap(),
            enable_preview_from_project_panel: preview_tabs
                .enable_preview_from_project_panel
                .unwrap(),
            enable_preview_from_file_finder: preview_tabs.enable_preview_from_file_finder.unwrap(),
            enable_preview_from_multibuffer: preview_tabs.enable_preview_from_multibuffer.unwrap(),
            enable_preview_multibuffer_from_code_navigation: preview_tabs
                .enable_preview_multibuffer_from_code_navigation
                .unwrap(),
            enable_preview_file_from_code_navigation: preview_tabs
                .enable_preview_file_from_code_navigation
                .unwrap(),
            enable_keep_preview_on_code_navigation: preview_tabs
                .enable_keep_preview_on_code_navigation
                .unwrap(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum ItemEvent {
    CloseItem,
    UpdateTab,
    UpdateBreadcrumbs,
    Edit,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TabContentParams {
    pub detail: Option<usize>,
    pub selected: bool,
    pub preview: bool,
    /// Tab content should be deemphasized when active pane does not have focus.
    pub deemphasized: bool,
    /// Maximum character length for the title. None = use the item's own default (typically MAX_TAB_TITLE_LEN).
    pub max_title_len: Option<usize>,
    pub truncate_title_middle: bool,
}

impl TabContentParams {
    /// Returns the text color to be used for the tab content.
    pub fn text_color(&self) -> Color {
        if self.deemphasized {
            if self.selected {
                Color::Muted
            } else {
                Color::Hidden
            }
        } else if self.selected {
            Color::Default
        } else {
            Color::Muted
        }
    }
}

pub fn tab_label_color(selected: bool) -> Color {
    if selected {
        Color::Default
    } else {
        Color::Muted
    }
}

pub enum TabTooltipContent {
    Text(SharedString),
    Custom(Box<dyn Fn(&mut Window, &mut App) -> AnyView>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemBufferKind {
    Multibuffer,
    Singleton,
    None,
}

pub trait Item: Focusable + EventEmitter<Self::Event> + Render + Sized {
    type Event;

    /// Returns the tab contents.
    ///
    /// By default this returns a [`Label`] that displays that text from
    /// `tab_content_text`.
    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> AnyElement {
        let text = self.tab_content_text(params.detail.unwrap_or_default(), cx);
        let overlay = self.tab_content_overlay(window, cx);
        let label = Label::new(text)
            .color(tab_label_color(params.selected))
            .when(overlay.is_some(), |this| this.alpha(0.));

        if let Some(overlay) = overlay {
            h_flex()
                .relative()
                .min_w_0()
                .child(label)
                .child(div().absolute().top_0().left_0().size_full().child(overlay))
                .into_any_element()
        } else {
            label.into_any_element()
        }
    }

    /// Returns the textual contents of the tab.
    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString;

    fn tab_content_overlay(&self, _window: &Window, _cx: &App) -> Option<AnyElement> {
        None
    }

    /// Returns the suggested filename for saving this item.
    /// By default, returns the tab content text.
    fn suggested_filename(&self, cx: &App) -> SharedString {
        self.tab_content_text(0, cx)
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        None
    }

    fn tab_icon_element(&self, _window: &Window, _cx: &App) -> Option<AnyElement> {
        None
    }

    fn tab_close_icon(&self, _cx: &App) -> IconName {
        IconName::Close
    }

    fn tab_close_tooltip_text(&self) -> &'static str {
        "Close Tab"
    }

    /// Returns the tab tooltip text.
    ///
    /// Use this if you don't need to customize the tab tooltip content.
    fn tab_tooltip_text(&self, _: &App) -> Option<SharedString> {
        None
    }

    /// Returns the tab tooltip content.
    ///
    /// By default this returns a Tooltip text from
    /// `tab_tooltip_text`.
    fn tab_tooltip_content(&self, cx: &App) -> Option<TabTooltipContent> {
        self.tab_tooltip_text(cx).map(TabTooltipContent::Text)
    }

    fn to_item_events(_event: &Self::Event, _f: &mut dyn FnMut(ItemEvent)) {}

    fn activated(&mut self, _window: &mut Window, _: &mut Context<Self>) {}
    fn deactivated(&mut self, _window: &mut Window, _: &mut Context<Self>) {}
    fn discarded(&self, _project: Entity<Project>, _window: &mut Window, _cx: &mut Context<Self>) {}
    fn on_removed(&self, _cx: &mut Context<Self>) {}
    fn on_close(
        &mut self,
        _save_intent: SaveIntent,
        _cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        Task::ready(Ok(true))
    }
    fn workspace_deactivated(&mut self, _window: &mut Window, _: &mut Context<Self>) {}
    fn pane_changed(&mut self, _new_pane_id: EntityId, _cx: &mut Context<Self>) {}
    fn navigate(
        &mut self,
        _: Arc<dyn Any + Send>,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) -> bool {
        false
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }

    /// (model id, Item)
    fn for_each_project_item(
        &self,
        _: &App,
        _: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
    }
    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::None
    }

    /// Returns the project path that should be treated as active for this item.
    ///
    /// Singleton items use their only project item by default. Items backed by
    /// multiple buffers should override this to return the path for the buffer
    /// under the primary cursor or otherwise selected sub-item.
    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        if self.buffer_kind(cx) != ItemBufferKind::Singleton {
            return None;
        }

        let mut result = None;
        self.for_each_project_item(cx, &mut |_, item| {
            result = item.project_path(cx);
        });
        result
    }

    fn set_nav_history(&mut self, _: ItemNavHistory, _window: &mut Window, _: &mut Context<Self>) {}

    fn can_split(&self) -> bool {
        false
    }
    fn clone_on_split(
        &self,
        workspace_id: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>>
    where
        Self: Sized,
    {
        _ = (workspace_id, window, cx);
        unimplemented!("clone_on_split() must be implemented if can_split() returns true")
    }
    fn is_dirty(&self, _: &App) -> bool {
        false
    }
    fn capability(&self, _: &App) -> Capability {
        Capability::ReadWrite
    }

    fn toggle_read_only(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn has_deleted_file(&self, _: &App) -> bool {
        false
    }
    fn has_conflict(&self, _: &App) -> bool {
        false
    }
    fn can_save(&self, _cx: &App) -> bool {
        false
    }
    fn can_save_as(&self, _: &App) -> bool {
        false
    }
    fn save(
        &mut self,
        _options: SaveOptions,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        unimplemented!("save() must be implemented if can_save() returns true")
    }
    fn save_as(
        &mut self,
        _project: Entity<Project>,
        _path: ProjectPath,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        unimplemented!("save_as() must be implemented if can_save() returns true")
    }
    fn reload(
        &mut self,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        unimplemented!("reload() must be implemented if can_save() returns true")
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<AnyEntity> {
        if TypeId::of::<Self>() == type_id {
            Some(self_handle.clone().into())
        } else {
            None
        }
    }

    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        None
    }

    fn breadcrumb_location(&self, _: &App) -> ToolbarItemLocation {
        ToolbarItemLocation::Hidden
    }

    fn breadcrumbs(&self, _cx: &App) -> Option<(Vec<HighlightedText>, Option<Font>)> {
        None
    }

    /// Returns optional elements to render to the left of the breadcrumb.
    fn breadcrumb_prefix(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        None
    }

    fn added_to_workspace(
        &mut self,
        _workspace: &mut Workspace,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn show_toolbar(&self) -> bool {
        true
    }

    fn pixel_position_of_cursor(&self, _: &App) -> Option<Point<Pixels>> {
        None
    }

    fn preserve_preview(&self, _cx: &App) -> bool {
        false
    }

    fn include_in_nav_history() -> bool {
        true
    }

    /// Called when the containing pane receives a drop on the item or the item's tab.
    /// Returns `true` to consume it and suppress the pane's default drop behavior.
    fn handle_drop(
        &self,
        _active_pane: &Pane,
        _dropped: &dyn Any,
        _window: &mut Window,
        _cx: &mut App,
    ) -> bool {
        false
    }

    /// Returns additional actions to add to the tab's context menu.
    /// Each entry is a label and an action to dispatch.
    fn tab_extra_context_menu_actions(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Vec<(SharedString, Box<dyn Action>)> {
        Vec::new()
    }
}

mod entity_item_handle;
mod entity_lifecycle;
mod followable;
mod item_handle_impls;
mod item_handle_traits;
mod project_item;
mod serializable;

pub use followable::{Dedup, FollowEvent, FollowableItem, FollowableItemHandle};
pub use item_handle_traits::{ItemHandle, WeakItemHandle};
pub use project_item::{ProjectItem, ProjectItemKind};
pub use serializable::{SerializableItem, SerializableItemHandle};

#[cfg(any(test, feature = "test-support"))]
pub mod test;
