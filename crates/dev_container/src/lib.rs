use std::path::Path;

use fs::Fs;
use gpui::AppContext;
use gpui::Entity;
use gpui::Task;
use gpui::WeakEntity;
use http_client::anyhow;
use picker::Picker;
use picker::PickerDelegate;
use project::ProjectEnvironment;
use settings::RegisterSetting;
use settings::Settings;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fmt::Display;
use std::sync::Arc;
use ui::ActiveTheme;
use ui::Button;
use ui::Clickable;
use ui::FluentBuilder;
use ui::KeyBinding;
use ui::StatefulInteractiveElement;
use ui::Switch;
use ui::ToggleState;
use ui::Tooltip;
use ui::h_flex;
use ui::rems_from_px;
use ui::v_flex;
use util::shell::Shell;

use gpui::{Action, DismissEvent, EventEmitter, FocusHandle, Focusable, RenderOnce};
use serde::Deserialize;
use ui::{
    AnyElement, App, Color, CommonAnimationExt, Context, Headline, HeadlineSize, Icon, IconName,
    InteractiveElement, IntoElement, Label, ListItem, ListSeparator, ModalHeader, Navigable,
    NavigableEntry, ParentElement, Render, Styled, StyledExt, Toggleable, Window, div, rems,
};
use util::ResultExt;
use util::rel_path::RelPath;
use workspace::{ModalView, Workspace, with_active_or_new_workspace};

use http_client::HttpClient;

mod command_json;
mod devcontainer_api;
mod devcontainer_json;
mod devcontainer_manifest;
mod docker;
mod features;
mod oci;

use devcontainer_api::read_default_devcontainer_configuration;

use crate::devcontainer_api::DevContainerError;
use crate::devcontainer_api::apply_devcontainer_template;
use crate::oci::get_deserializable_oci_blob;
use crate::oci::get_latest_oci_manifest;
use crate::oci::get_oci_token;

pub use devcontainer_api::{
    DevContainerConfig, find_configs_in_snapshot, find_devcontainer_configs,
    start_dev_container_with_config,
};

#[path = "lib/apply.rs"]
mod apply;
#[path = "lib/feature_picker.rs"]
mod feature_picker;
#[path = "lib/modal_render.rs"]
mod modal_render;
#[path = "lib/modal_state.rs"]
mod modal_state;
#[path = "lib/modal_traits.rs"]
mod modal_traits;
#[path = "lib/registry_fetch.rs"]
mod registry_fetch;
#[path = "lib/template_picker.rs"]
mod template_picker;

use apply::*;
use feature_picker::*;
use modal_traits::*;
pub(crate) use registry_fetch::*;
use template_picker::*;

#[cfg(test)]
mod registry_tests;

/// Converts a string to a safe environment variable name.
///
/// Mirrors the CLI's `getSafeId` in `containerFeatures.ts`:
/// replaces non-alphanumeric/underscore characters with `_`, replaces a
/// leading sequence of digits/underscores with a single `_`, and uppercases.
pub(crate) fn safe_id_lower(input: &str) -> String {
    get_safe_id(input).to_lowercase()
}
pub(crate) fn safe_id_upper(input: &str) -> String {
    get_safe_id(input).to_uppercase()
}
fn get_safe_id(input: &str) -> String {
    let replaced: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let without_leading = replaced.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    let result = if without_leading.len() < replaced.len() {
        format!("_{}", without_leading)
    } else {
        replaced
    };
    result
}

pub struct DevContainerContext {
    pub project_directory: Arc<Path>,
    pub use_podman: bool,
    pub use_buildkit: Option<bool>,
    pub fs: Arc<dyn Fs>,
    pub http_client: Arc<dyn HttpClient>,
    pub environment: WeakEntity<ProjectEnvironment>,
}

impl DevContainerContext {
    pub fn from_workspace(workspace: &Workspace, cx: &App) -> Option<Self> {
        let project_directory = workspace.project().read(cx).active_project_directory(cx)?;
        let settings = DevContainerSettings::get_global(cx);
        let use_podman = settings.use_podman;
        let use_buildkit = settings.use_buildkit;
        let http_client = cx.http_client().clone();
        let fs = workspace.app_state().fs.clone();
        let environment = workspace.project().read(cx).environment().downgrade();
        Some(Self {
            project_directory,
            use_podman,
            use_buildkit,
            fs,
            http_client,
            environment,
        })
    }

    pub async fn environment(&self, cx: &mut impl AppContext) -> HashMap<String, String> {
        let Ok(task) = self.environment.update(cx, |this, cx| {
            this.local_directory_environment(&Shell::System, self.project_directory.clone(), cx)
        }) else {
            return HashMap::default();
        };
        task.await
            .map(|env| env.into_iter().collect::<std::collections::HashMap<_, _>>())
            .unwrap_or_default()
    }
}

#[derive(RegisterSetting)]
struct DevContainerSettings {
    use_podman: bool,
    use_buildkit: Option<bool>,
}

pub fn use_podman(cx: &App) -> bool {
    DevContainerSettings::get_global(cx).use_podman
}

impl Settings for DevContainerSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            use_podman: content.remote.use_podman.unwrap_or(false),
            use_buildkit: content.remote.dev_container_use_buildkit,
        }
    }
}

#[derive(PartialEq, Clone, Deserialize, Default, Action)]
#[action(namespace = projects)]
#[serde(deny_unknown_fields)]
struct InitializeDevContainer;

pub fn init(cx: &mut App) {
    cx.on_action(|_: &InitializeDevContainer, cx| {
        with_active_or_new_workspace(cx, move |workspace, window, cx| {
            let weak_entity = cx.weak_entity();
            workspace.toggle_modal(window, cx, |window, cx| {
                DevContainerModal::new(weak_entity, window, cx)
            });
        });
    });
}

fn ghcr_registry() -> &'static str {
    "ghcr.io"
}

fn devcontainer_templates_repository() -> &'static str {
    "devcontainers/templates"
}

fn devcontainer_features_repository() -> &'static str {
    "devcontainers/features"
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TemplateOptions {
    #[serde(rename = "type")]
    option_type: String,
    description: Option<String>,
    proposals: Option<Vec<String>>,
    #[serde(rename = "enum")]
    enum_values: Option<Vec<String>>,
    // Different repositories surface "default: 'true'" or "default: true",
    // so we need to be flexible in deserializing
    #[serde(deserialize_with = "deserialize_string_or_bool")]
    default: String,
}

fn deserialize_string_or_bool<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrBool {
        String(String),
        Bool(bool),
    }

    match StringOrBool::deserialize(deserializer)? {
        StringOrBool::String(s) => Ok(s),
        StringOrBool::Bool(b) => Ok(b.to_string()),
    }
}

impl TemplateOptions {
    fn possible_values(&self) -> Vec<String> {
        match self.option_type.as_str() {
            "string" => self
                .enum_values
                .clone()
                .or(self.proposals.clone().or(Some(vec![self.default.clone()])))
                .unwrap_or_default(),
            // If not string, must be boolean
            _ => {
                if self.default == "true" {
                    vec!["true".to_string(), "false".to_string()]
                } else {
                    vec!["false".to_string(), "true".to_string()]
                }
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
struct DevContainerFeature {
    id: String,
    version: String,
    name: String,
    source_repository: Option<String>,
}

impl DevContainerFeature {
    fn major_version(&self) -> String {
        let Some(mv) = self.version.get(..1) else {
            return "".to_string();
        };
        mv.to_string()
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DevContainerTemplate {
    id: String,
    name: String,
    options: Option<HashMap<String, TemplateOptions>>,
    source_repository: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevContainerFeaturesResponse {
    features: Vec<DevContainerFeature>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevContainerTemplatesResponse {
    templates: Vec<DevContainerTemplate>,
}

#[derive(Clone)]
struct TemplateEntry {
    template: DevContainerTemplate,
    options_selected: HashMap<String, String>,
    current_option_index: usize,
    current_option: Option<TemplateOptionSelection>,
    features_selected: HashSet<DevContainerFeature>,
}

#[derive(Clone)]
struct FeatureEntry {
    feature: DevContainerFeature,
    toggle_state: ToggleState,
}

#[derive(Clone)]
struct TemplateOptionSelection {
    option_name: String,
    description: String,
    navigable_options: Vec<(String, NavigableEntry)>,
}

impl Eq for TemplateEntry {}
impl PartialEq for TemplateEntry {
    fn eq(&self, other: &Self) -> bool {
        self.template == other.template
    }
}
impl Debug for TemplateEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateEntry")
            .field("template", &self.template)
            .finish()
    }
}

impl Eq for FeatureEntry {}
impl PartialEq for FeatureEntry {
    fn eq(&self, other: &Self) -> bool {
        self.feature == other.feature
    }
}

impl Debug for FeatureEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureEntry")
            .field("feature", &self.feature)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DevContainerState {
    Initial,
    QueryingTemplates,
    TemplateQueryReturned(Result<Vec<TemplateEntry>, String>),
    QueryingFeatures(TemplateEntry),
    FeaturesQueryReturned(TemplateEntry),
    UserOptionsSpecifying(TemplateEntry),
    ConfirmingWriteDevContainer(TemplateEntry),
    TemplateWriteFailed(DevContainerError),
}

#[derive(Debug, Clone)]
enum DevContainerMessage {
    SearchTemplates,
    TemplatesRetrieved(Vec<DevContainerTemplate>),
    ErrorRetrievingTemplates(String),
    TemplateSelected(TemplateEntry),
    TemplateOptionsSpecified(TemplateEntry),
    TemplateOptionsCompleted(TemplateEntry),
    FeaturesRetrieved(Vec<DevContainerFeature>),
    FeaturesSelected(TemplateEntry),
    NeedConfirmWriteDevContainer(TemplateEntry),
    ConfirmWriteDevContainer(TemplateEntry),
    FailedToWriteTemplate(DevContainerError),
    GoBack,
}

struct DevContainerModal {
    workspace: WeakEntity<Workspace>,
    picker: Option<Entity<Picker<TemplatePickerDelegate>>>,
    features_picker: Option<Entity<Picker<FeaturePickerDelegate>>>,
    focus_handle: FocusHandle,
    confirm_entry: NavigableEntry,
    back_entry: NavigableEntry,
    state: DevContainerState,
}
