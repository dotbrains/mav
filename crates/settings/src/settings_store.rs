use anyhow::{Context as _, Result};
use collections::{BTreeMap, HashMap, TypeIdHashMap, btree_map, hash_map};
use fs::Fs;
use futures::{
    FutureExt, StreamExt,
    channel::{mpsc, oneshot},
    future::LocalBoxFuture,
};
use gpui::{
    App, AppContext, AsyncApp, BorrowAppContext, Entity, Global, SharedString, Task, UpdateGlobal,
};

use paths::{local_settings_file_relative_path, task_file_name};
use schemars::{JsonSchema, json_schema};
use serde_json::Value;
use settings_content::{CommandAliasTarget, ParseStatus};
use std::{
    any::{Any, TypeId, type_name},
    fmt::Debug,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    str,
    sync::Arc,
};
use util::{
    ResultExt as _,
    rel_path::RelPath,
    schemars::{AllowTrailingCommas, DefaultDenyUnknownFields, replace_subschema},
};

use crate::editorconfig_store::EditorconfigStore;

use crate::{
    ActiveSettingsProfileName, FontFamilyName, IconThemeName, LanguageSettingsContent,
    LanguageToSettingsMap, LspSettings, LspSettingsMap, SemanticTokenRules, ThemeName,
    UserSettingsContentExt, VsCodeSettings, WorktreeId,
    settings_content::{
        ExtensionsSettingsContent, ProfileBase, ProjectSettingsContent, RootUserSettings,
        SettingsContent, UserSettingsContent, merge_from::MergeFrom,
    },
};

use settings_json::{infer_json_indent_size, update_value_in_json_text};

pub const LSP_SETTINGS_SCHEMA_URL_PREFIX: &str = "mav://schemas/settings/lsp/";

pub trait SettingsKey: 'static + Send + Sync {
    /// The name of a key within the JSON file from which this setting should
    /// be deserialized. If this is `None`, then the setting will be deserialized
    /// from the root object.
    const KEY: Option<&'static str>;

    const FALLBACK_KEY: Option<&'static str> = None;
}

/// A value that can be defined as a user setting.
///
/// Settings can be loaded from a combination of multiple JSON files.
pub trait Settings: 'static + Send + Sync + Sized {
    /// The name of the keys in the [`FileContent`](Self::FileContent) that should
    /// always be written to a settings file, even if their value matches the default
    /// value.
    ///
    /// This is useful for tagged [`FileContent`](Self::FileContent)s where the tag
    /// is a "version" field that should always be persisted, even if the current
    /// user settings match the current version of the settings.
    const PRESERVED_KEYS: Option<&'static [&'static str]> = None;

    /// Read the value from default.json.
    ///
    /// This function *should* panic if default values are missing,
    /// and you should add a default to default.json for documentation.
    fn from_settings(content: &SettingsContent) -> Self;

    #[track_caller]
    fn register(cx: &mut App)
    where
        Self: Sized,
    {
        SettingsStore::update_global(cx, |store, _| {
            store.register_setting::<Self>();
        });
    }

    #[track_caller]
    fn get<'a>(path: Option<SettingsLocation>, cx: &'a App) -> &'a Self
    where
        Self: Sized,
    {
        cx.global::<SettingsStore>().get(path)
    }

    #[track_caller]
    fn get_global(cx: &App) -> &Self
    where
        Self: Sized,
    {
        cx.global::<SettingsStore>().get(None)
    }

    #[track_caller]
    fn try_get(cx: &App) -> Option<&Self>
    where
        Self: Sized,
    {
        if cx.has_global::<SettingsStore>() {
            cx.global::<SettingsStore>().try_get(None)
        } else {
            None
        }
    }

    #[track_caller]
    fn try_read_global<R>(cx: &AsyncApp, f: impl FnOnce(&Self) -> R) -> Option<R>
    where
        Self: Sized,
    {
        cx.try_read_global(|s: &SettingsStore, _| f(s.get(None)))
    }

    #[track_caller]
    fn override_global(settings: Self, cx: &mut App)
    where
        Self: Sized,
    {
        cx.global_mut::<SettingsStore>().override_global(settings)
    }
}

pub struct RegisteredSetting {
    pub settings_value: fn() -> Box<dyn AnySettingValue>,
    pub from_settings: fn(&SettingsContent) -> Box<dyn Any>,
    pub id: fn() -> TypeId,
}

inventory::collect!(RegisteredSetting);

#[derive(Clone, Copy, Debug)]
pub struct SettingsLocation<'a> {
    pub worktree_id: WorktreeId,
    pub path: &'a RelPath,
}

pub struct SettingsStore {
    setting_values: TypeIdHashMap<Box<dyn AnySettingValue>>,
    default_settings: Rc<SettingsContent>,
    user_settings: Option<UserSettingsContent>,
    global_settings: Option<Box<SettingsContent>>,

    extension_settings: Option<Box<SettingsContent>>,
    server_settings: Option<Box<SettingsContent>>,

    language_semantic_token_rules: HashMap<SharedString, SemanticTokenRules>,

    merged_settings: Rc<SettingsContent>,

    last_user_settings_content: Option<String>,
    last_global_settings_content: Option<String>,
    local_settings: BTreeMap<(WorktreeId, Arc<RelPath>), SettingsContent>,
    pub editorconfig_store: Entity<EditorconfigStore>,

    _settings_files_watcher: Option<Task<()>>,
    _setting_file_updates: Task<()>,
    setting_file_updates_tx:
        mpsc::UnboundedSender<Box<dyn FnOnce(AsyncApp) -> LocalBoxFuture<'static, Result<()>>>>,
    file_errors: BTreeMap<SettingsFile, SettingsParseResult>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SettingsFile {
    Default,
    Global,
    User,
    Server,
    /// Represents project settings in ssh projects as well as local projects
    Project((WorktreeId, Arc<RelPath>)),
}

impl PartialOrd for SettingsFile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Sorted in order of precedence
impl Ord for SettingsFile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use SettingsFile::*;
        use std::cmp::Ordering;
        match (self, other) {
            (User, User) => Ordering::Equal,
            (Server, Server) => Ordering::Equal,
            (Default, Default) => Ordering::Equal,
            (Project((id1, rel_path1)), Project((id2, rel_path2))) => id1
                .cmp(id2)
                .then_with(|| rel_path1.cmp(rel_path2).reverse()),
            (Project(_), _) => Ordering::Less,
            (_, Project(_)) => Ordering::Greater,
            (Server, _) => Ordering::Less,
            (_, Server) => Ordering::Greater,
            (User, _) => Ordering::Less,
            (_, User) => Ordering::Greater,
            (Global, _) => Ordering::Less,
            (_, Global) => Ordering::Greater,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocalSettingsKind {
    Settings,
    Tasks,
    Editorconfig,
    Debug,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocalSettingsPath {
    InWorktree(Arc<RelPath>),
    OutsideWorktree(Arc<Path>),
}

impl LocalSettingsPath {
    pub fn is_outside_worktree(&self) -> bool {
        matches!(self, Self::OutsideWorktree(_))
    }

    pub fn to_proto(&self) -> String {
        match self {
            Self::InWorktree(path) => path.to_proto(),
            Self::OutsideWorktree(path) => path.to_string_lossy().to_string(),
        }
    }

    pub fn from_proto(path: &str, is_outside_worktree: bool) -> anyhow::Result<Self> {
        if is_outside_worktree {
            Ok(Self::OutsideWorktree(PathBuf::from(path).into()))
        } else {
            Ok(Self::InWorktree(RelPath::from_proto(path)?))
        }
    }
}

impl Global for SettingsStore {}

#[derive(Default)]
pub struct DefaultSemanticTokenRules(pub SemanticTokenRules);

impl gpui::Global for DefaultSemanticTokenRules {}

#[doc(hidden)]
#[derive(Debug)]
pub struct SettingValue<T> {
    #[doc(hidden)]
    pub global_value: Option<T>,
    #[doc(hidden)]
    pub local_values: Vec<(WorktreeId, Arc<RelPath>, T)>,
}

#[doc(hidden)]
pub trait AnySettingValue: 'static + Send + Sync {
    fn setting_type_name(&self) -> &'static str;

    fn from_settings(&self, s: &SettingsContent) -> Box<dyn Any>;

    fn value_for_path(&self, path: Option<SettingsLocation>) -> &dyn Any;
    fn all_local_values(&self) -> Vec<(WorktreeId, Arc<RelPath>, &dyn Any)>;
    fn set_global_value(&mut self, value: Box<dyn Any>);
    fn set_local_value(&mut self, root_id: WorktreeId, path: Arc<RelPath>, value: Box<dyn Any>);
    fn clear_local_values(&mut self, root_id: WorktreeId);
}

/// Parameters that are used when generating some JSON schemas at runtime.
pub struct SettingsJsonSchemaParams<'a> {
    pub language_names: &'a [String],
    pub font_names: &'a [String],
    pub theme_names: &'a [SharedString],
    pub icon_theme_names: &'a [SharedString],
    pub lsp_adapter_names: &'a [String],
    pub action_names: &'a [&'a str],
    pub action_documentation: &'a HashMap<&'a str, &'a str>,
    pub deprecations: &'a HashMap<&'a str, &'a str>,
    pub deprecation_messages: &'a HashMap<&'a str, &'a str>,
}

#[path = "settings_store/file_io.rs"]
mod file_io;
#[path = "settings_store/json_updates.rs"]
mod json_updates;
#[path = "settings_store/parse_result.rs"]
mod parse_result;
pub use parse_result::{InvalidSettingsError, MigrationStatus, SettingsParseResult};
#[path = "settings_store/recompute.rs"]
mod recompute;
#[path = "settings_store/schema.rs"]
mod schema;
#[path = "settings_store/setters.rs"]
mod setters;
#[path = "settings_store/store_core.rs"]
mod store_core;
#[path = "settings_store/tests_basic.rs"]
#[cfg(test)]
mod tests_basic;
#[path = "settings_store/tests_common.rs"]
#[cfg(test)]
mod tests_common;
#[path = "settings_store/tests_file_io.rs"]
#[cfg(test)]
mod tests_file_io;
#[path = "settings_store/tests_schema.rs"]
#[cfg(test)]
mod tests_schema;
#[path = "settings_store/tests_updates.rs"]
#[cfg(test)]
mod tests_updates;
#[path = "settings_store/tests_values.rs"]
#[cfg(test)]
mod tests_values;
#[path = "settings_store/tests_vscode.rs"]
#[cfg(test)]
mod tests_vscode;
#[path = "settings_store/value_impl.rs"]
mod value_impl;
