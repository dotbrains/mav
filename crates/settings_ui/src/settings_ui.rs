mod active_language;
mod components;
#[path = "settings_ui/navigation.rs"]
mod navigation;
mod page_data;
#[path = "settings_ui/page_model.rs"]
mod page_model;
pub mod pages;
#[path = "settings_ui/renderer_registration.rs"]
mod renderer_registration;
mod setting_field;
mod setting_field_renderers;
mod settings_file_target;
mod settings_file_types;
mod settings_file_updates;
#[cfg(test)]
mod settings_file_updates_tests;
mod settings_item_rendering;

use agent_skills::SkillIndex;
use anyhow::{Context as _, Result};
use cloud_api_types::OrganizationConfiguration;
use editor::{Editor, EditorEvent};
use futures::{StreamExt, channel::mpsc};
use fuzzy::StringMatchCandidate;
use gpui::{
    Action, App, AsyncApp, ClipboardItem, DEFAULT_ADDITIONAL_WINDOW_SIZE, Div, Entity, FocusHandle,
    Focusable, Global, KeyContext, ListState, ReadGlobal as _, Role, ScrollHandle, Stateful,
    Subscription, Task, Tiling, TitlebarOptions, UniformListScrollHandle, WeakEntity, Window,
    WindowBounds, WindowHandle, WindowOptions, actions, div, list, point, prelude::*, px,
    uniform_list,
};
use heck::ToTitleCase as _;

use language::Buffer;
use platform_title_bar::PlatformTitleBar;
use project::{Project, ProjectPath, Worktree, WorktreeId};
use release_channel::ReleaseChannel;
use schemars::JsonSchema;
use serde::Deserialize;
use settings::{
    IntoGpui, Settings, SettingsContent, SettingsStore, initial_project_settings_content,
};
use std::{
    any::{Any, TypeId, type_name},
    cell::RefCell,
    collections::{HashMap, HashSet},
    num::{NonZero, NonZeroU32},
    ops::Range,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use theme_settings::ThemeSettings;
use ui::{
    Banner, ContextMenu, Divider, DropdownMenu, DropdownStyle, IconButtonShape, KeyBinding,
    KeybindingHint, PopoverMenu, Scrollbars, Switch, Tooltip, TreeViewItem, WithScrollbar,
    prelude::*,
};

use mav_actions::{AGENT_SKILLS_SETTINGS_PATH, OpenProjectSettings, OpenSettings, OpenSettingsAt};
use util::{ResultExt as _, paths::PathStyle, rel_path::RelPath};
use workspace::{
    AppState, MultiWorkspace, OpenOptions, OpenVisible, Workspace, WorkspaceSettings,
    client_side_decorations,
};

pub(crate) use crate::active_language::active_language;
use crate::active_language::active_language_mut;
use crate::components::{
    EnumVariantDropdown, NumberField, NumberFieldMode, NumberFieldType, SettingsInputField,
    SettingsSectionHeader, font_picker, icon_theme_picker, render_ollama_model_picker,
    text_field_a11y_state, theme_picker,
};
pub(crate) use crate::page_model::{
    ActionLink, DynamicItem, NavBarEntry, SearchIndex, SettingItem, SettingsPage, SettingsPageItem,
    SubPage, SubPageLink, all_language_names,
};
use crate::pages::{
    CustomAgentForm, McpServerForm, render_input_audio_device_dropdown,
    render_output_audio_device_dropdown,
};
use crate::setting_field::{
    AnySettingField, NonFocusableHandle, SettingField, SettingFieldRenderer, SettingsFieldMetadata,
    UnimplementedSettingField,
};
pub(crate) use crate::setting_field_renderers::render_picker_trigger_button;
use crate::setting_field_renderers::{
    render_dropdown, render_editable_number_field, render_font_picker, render_icon_theme_picker,
    render_text_field, render_theme_picker, render_toggle_button,
};
use crate::settings_file_target::SettingsFileTarget;
use crate::settings_file_types::FileMask;
pub(crate) use crate::settings_file_types::{PROJECT, SERVER, SettingsUiFile, USER};
use crate::settings_file_updates::{
    ProjectSettingsUpdateQueue, open_user_settings_in_workspace, update_settings_file,
};
pub(crate) use crate::settings_item_rendering::{
    render_settings_item, render_settings_item_layout, render_settings_item_link,
};

const NAVBAR_CONTAINER_TAB_INDEX: isize = 0;
const NAVBAR_GROUP_TAB_INDEX: isize = 1;

const HEADER_CONTAINER_TAB_INDEX: isize = 2;
const HEADER_GROUP_TAB_INDEX: isize = 3;

const CONTENT_CONTAINER_TAB_INDEX: isize = 4;
const CONTENT_GROUP_TAB_INDEX: isize = 5;

actions!(
    settings_editor,
    [
        /// Minimizes the settings UI window.
        Minimize,
        /// Toggles focus between the navbar and the main content.
        ToggleFocusNav,
        /// Expands the navigation entry.
        ExpandNavEntry,
        /// Collapses the navigation entry.
        CollapseNavEntry,
        /// Focuses the next file in the file list.
        FocusNextFile,
        /// Focuses the previous file in the file list.
        FocusPreviousFile,
        /// Opens an editor for the current file
        OpenCurrentFile,
        /// Focuses the previous root navigation entry.
        FocusPreviousRootNavEntry,
        /// Focuses the next root navigation entry.
        FocusNextRootNavEntry,
        /// Focuses the first navigation entry.
        FocusFirstNavEntry,
        /// Focuses the last navigation entry.
        FocusLastNavEntry,
        /// Focuses and opens the next navigation entry without moving focus to content.
        FocusNextNavEntry,
        /// Focuses and opens the previous navigation entry without moving focus to content.
        FocusPreviousNavEntry
    ]
);

#[derive(Action, PartialEq, Eq, Clone, Copy, Debug, JsonSchema, Deserialize)]
#[action(namespace = settings_editor)]
struct FocusFile(pub u32);

#[path = "settings_ui/file_header.rs"]
mod file_header;
#[path = "settings_ui/navigation_render.rs"]
mod navigation_render;
#[path = "settings_ui/open.rs"]
mod open;
#[path = "settings_ui/page_items.rs"]
mod page_items;
#[path = "settings_ui/page_render.rs"]
mod page_render;
#[path = "settings_ui/render.rs"]
mod render;
#[path = "settings_ui/search_and_files.rs"]
mod search_and_files;
#[path = "settings_ui/sub_pages_and_focus.rs"]
mod sub_pages_and_focus;
#[path = "settings_ui/window_lifecycle.rs"]
mod window_lifecycle;
pub struct SettingsWindow {
    title_bar: Option<Entity<PlatformTitleBar>>,
    original_window: Option<WindowHandle<MultiWorkspace>>,
    files: Vec<(SettingsUiFile, FocusHandle)>,
    worktree_root_dirs: HashMap<WorktreeId, String>,
    current_file: SettingsUiFile,
    pages: Vec<SettingsPage>,
    sub_page_stack: Vec<SubPage>,
    opening_link: bool,
    search_bar: Entity<Editor>,
    search_task: Option<Task<()>>,
    /// Cached settings file buffers to avoid repeated disk I/O on each settings change
    project_setting_file_buffers: HashMap<ProjectPath, Entity<Buffer>>,
    /// Index into navbar_entries
    navbar_entry: usize,
    navbar_entries: Vec<NavBarEntry>,
    navbar_scroll_handle: UniformListScrollHandle,
    /// [page_index][page_item_index] will be false
    /// when the item is filtered out either by searches
    /// or by the current file
    navbar_focus_subscriptions: Vec<gpui::Subscription>,
    filter_table: Vec<Vec<bool>>,
    has_query: bool,
    content_handles: Vec<Vec<Entity<NonFocusableHandle>>>,
    focus_handle: FocusHandle,
    navbar_focus_handle: Entity<NonFocusableHandle>,
    content_focus_handle: Entity<NonFocusableHandle>,
    files_focus_handle: FocusHandle,
    search_index: Option<Arc<SearchIndex>>,
    list_state: ListState,
    shown_errors: HashSet<String>,
    pub(crate) hidden_deleted_skill_directory_paths: HashSet<PathBuf>,
    pub(crate) regex_validation_error: Option<String>,
    pub(crate) sandbox_host_validation_error: Option<String>,
    last_copied_link_path: Option<&'static str>,
    /// Cached configuration views per provider, created lazily. Holds the
    /// provider's chosen presentation ([`Inline`] or [`SubPage`]).
    pub(crate) provider_configuration_views:
        HashMap<language_model::LanguageModelProviderId, language_model::ProviderConfigurationView>,
    /// The provider whose configuration sub-page is currently open, if any.
    pub(crate) configuring_provider: Option<language_model::LanguageModelProviderId>,
    /// Directory path of the skill whose share link was most recently copied,
    /// used to show a transient "copied" checkmark on its share button.
    pub(crate) last_copied_skill_directory_path: Option<PathBuf>,
    /// State for the active "add/edit custom MCP server" form sub-page, if open.
    pub(crate) mcp_server_form: Option<McpServerForm>,
    /// Stable focus handle for the MCP "Add Server" button, so it can show a
    /// focus ring when the page auto-focuses it on open (which happens via mouse,
    /// where `focus_visible` styling would otherwise be suppressed).
    pub(crate) mcp_add_server_focus_handle: FocusHandle,
    /// State for the active "add/edit custom external agent" form sub-page, if open.
    pub(crate) custom_agent_form: Option<CustomAgentForm>,
    /// Stable focus handle for the external agents "Add Agent" button, so it can
    /// show a focus ring when the page auto-focuses it on open (which happens via
    /// mouse, where `focus_visible` styling would otherwise be suppressed).
    pub(crate) external_agent_add_focus_handle: FocusHandle,
    skill_creator_page: Option<(Entity<pages::SkillCreatorPage>, Subscription)>,
}

pub(crate) fn all_projects(
    window: Option<&WindowHandle<MultiWorkspace>>,
    cx: &App,
) -> impl Iterator<Item = Entity<Project>> {
    let mut seen_project_ids = std::collections::HashSet::new();
    let app_state = workspace::AppState::global(cx);
    app_state
        .workspace_store
        .read(cx)
        .workspaces()
        .filter_map(|weak| weak.upgrade())
        .map(|workspace: Entity<Workspace>| workspace.read(cx).project().clone())
        .chain(
            window
                .and_then(|handle| handle.read(cx).ok())
                .into_iter()
                .flat_map(|multi_workspace| {
                    multi_workspace
                        .workspaces()
                        .map(|workspace| workspace.read(cx).project().clone())
                        .collect::<Vec<_>>()
                }),
        )
        .filter(move |project| seen_project_ids.insert(project.entity_id()))
}

#[cfg(test)]
pub mod test;
