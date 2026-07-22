use anyhow::{Context as _, Result};
use collections::{BTreeMap, HashMap, IndexMap};
use fs::Fs;
use gpui::{
    Action, ActionBuildError, App, InvalidKeystrokeError, KEYSTROKE_PARSE_EXPECTED_MESSAGE,
    KeyBinding, KeyBindingContextPredicate, KeyBindingMetaIndex, KeybindingKeystroke, Keystroke,
    NoAction, SharedString, Unbind, generate_list_of_all_registered_actions, register_action,
};
use schemars::{JsonSchema, json_schema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::{any::TypeId, fmt::Write, rc::Rc, sync::Arc, sync::LazyLock};
use util::ResultExt as _;
use util::{
    asset_str,
    markdown::{MarkdownEscaped, MarkdownInlineCode, MarkdownString},
    schemars::AllowTrailingCommas,
};

use crate::SettingsAssets;
use settings_content::{ActionName, ActionWithArguments};
use settings_json::{
    append_top_level_array_value_in_json_text, parse_json_with_comments,
    replace_top_level_array_value_in_json_text,
};

mod action_build;
mod action_sequence;
mod load;
mod schema;
#[cfg(test)]
mod tests;
mod types;
mod update;
mod update_types;

pub use action_sequence::ActionSequence;
pub use types::{
    KEY_BINDING_VALIDATORS, KeyBindingValidator, KeyBindingValidatorRegistration, KeymapAction,
    KeymapFile, KeymapFileLoadResult, KeymapSection, UnbindTargetAction,
};
pub use update_types::{KeybindSource, KeybindUpdateOperation, KeybindUpdateTarget};
