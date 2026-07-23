use std::path::PathBuf;

use super::*;
use crate::item::test::TestItem;
use agent_settings::AgentSettings;
use client::proto;
use fs::{FakeFs, Fs};
use gpui::{TestAppContext, VisualTestContext};
use project::DisableAiSettings;
use serde_json::json;
use settings::{Settings, SettingsStore};
use util::path;

mod close_and_switch;
mod open_project;
mod opening;
mod project_group_keys;
mod remote_groups;
mod settings;
mod test_support;
