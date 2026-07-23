use anyhow::Result;
use client::{Client, UserStore, mav_urls};
use cloud_llm_client::UsageLimit;
use codestral::{self, CodestralEditPredictionDelegate};
use copilot::Status;
use edit_prediction::EditPredictionStore;
use edit_prediction_types::EditPredictionDelegateHandle;
use editor::{
    Editor, MultiBufferOffset, SelectionEffects, actions::ShowEditPrediction, scroll::Autoscroll,
};
use feature_flags::FeatureFlagAppExt;
use fs::Fs;
use gpui::{
    Action, Anchor, Animation, AnimationExt, App, AsyncWindowContext, Entity, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, Subscription, TaskExt, WeakEntity, actions, div,
    ease_in_out, pulsating_between,
};
use indoc::indoc;
use language::{
    EditPredictionsMode, File, Language,
    language_settings::{
        AllLanguageSettings, EditPredictionProvider, LanguageSettings, all_language_settings,
    },
};
use project::{DisableAiSettings, Project};
use regex::Regex;
use settings::{Settings, SettingsStore, update_settings_file};
use std::{
    rc::Rc,
    sync::{Arc, LazyLock},
    time::Duration,
};
use ui::{
    Clickable, ContextMenu, ContextMenuEntry, DocumentationSide, IconButton, IconButtonShape,
    Indicator, PopoverMenu, PopoverMenuHandle, ProgressBar, Tooltip, prelude::*,
};
use util::ResultExt as _;

use mav_actions::{OpenBrowser, OpenSettingsAt};
use workspace::{
    HideStatusItem, StatusItemView, Toast, Workspace, create_and_open_local_file, item::ItemHandle,
    notifications::NotificationId,
};

use crate::{RatePredictions, rate_prediction_modal::PredictEditsRatePredictionsFeatureFlag};

actions!(
    edit_prediction,
    [
        /// Toggles the edit prediction menu.
        ToggleMenu
    ]
);

const COPILOT_SETTINGS_PATH: &str = "/settings/copilot";
const COPILOT_SETTINGS_URL: &str = concat!("https://github.com", "/settings/copilot");
const PRIVACY_DOCS: &str = "https://mav.dev/docs/ai/privacy-and-security";

struct CopilotErrorToast;

pub struct EditPredictionButton {
    editor_subscription: Option<(Subscription, usize)>,
    editor_enabled: Option<bool>,
    editor_show_predictions: bool,
    editor_focus_handle: Option<FocusHandle>,
    language: Option<Arc<Language>>,
    file: Option<Arc<dyn File>>,
    edit_prediction_provider: Option<Arc<dyn EditPredictionDelegateHandle>>,
    fs: Arc<dyn Fs>,
    user_store: Entity<UserStore>,
    popover_menu_handle: PopoverMenuHandle<ContextMenu>,
    project: WeakEntity<Project>,
}

#[path = "edit_prediction_button/construction.rs"]
mod construction;
#[path = "edit_prediction_button/helpers.rs"]
mod helpers;
#[path = "edit_prediction_button/language_menu.rs"]
mod language_menu;
#[path = "edit_prediction_button/provider_context_menu.rs"]
mod provider_context_menu;
#[path = "edit_prediction_button/provider_menu.rs"]
mod provider_menu;
#[path = "edit_prediction_button/render.rs"]
mod render;
#[path = "edit_prediction_button/state.rs"]
mod state;
#[path = "edit_prediction_button/status_item.rs"]
mod status_item;

pub(crate) use helpers::*;
pub use helpers::{get_available_providers, set_completion_provider};
pub(crate) use status_item::open_disabled_globs_setting_in_editor;

#[cfg(test)]
#[path = "edit_prediction_button/tests.rs"]
mod tests;
