use anyhow::{Context as _, Result};
use async_trait::async_trait;
use collections::HashMap;
use futures::StreamExt;
use futures::lock::OwnedMutexGuard;
use gpui::{App, AppContext, AsyncApp, Entity, SharedString, Task};
use http_client::github::AssetKind;
use http_client::github::{GitHubLspBinaryVersion, latest_github_release};
use http_client::github_download::{GithubBinaryMetadata, download_server_binary};
pub use language::*;
use lsp::{InitializeParams, LanguageServerBinary, LanguageServerBinaryOptions};
use project::lsp_store::lsp_ext_command;
use project::lsp_store::rust_analyzer_ext::CARGO_DIAGNOSTICS_SOURCE_NAME;
use project::project_settings::ProjectSettings;
use regex::Regex;
use serde_json::json;
use settings::{SemanticTokenRules, Settings as _};
use smallvec::SmallVec;
use smol::fs::{self};
use std::cmp::Reverse;
use std::fmt::Display;
use std::future::Future;
use std::ops::Range;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};
use task::{TaskTemplate, TaskTemplates, TaskVariables, VariableName};
use util::command::{Stdio, new_command};
use util::fs::{make_file_executable, remove_matching};
use util::merge_json_value_into;
use util::rel_path::RelPath;
use util::{ResultExt, maybe};

use crate::language_settings::LanguageSettings;

mod adapter;
mod binary;
mod context;
mod installer;
mod lsp_adapter;
mod manifest;
mod metadata;
#[cfg(test)]
mod tests;

use binary::get_cached_server_binary;
pub(crate) use context::RustContextProvider;
pub(crate) use manifest::CargoManifestProvider;
use metadata::{human_readable_package_name, target_info_from_abs_path};

pub(crate) fn semantic_token_rules() -> SemanticTokenRules {
    let content = grammars::get_file("rust/semantic_token_rules.json")
        .expect("missing rust/semantic_token_rules.json");
    let json = std::str::from_utf8(&content.data).expect("invalid utf-8 in semantic_token_rules");
    settings::parse_json_with_comments::<SemanticTokenRules>(json)
        .expect("failed to parse rust semantic_token_rules.json")
}

const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("rust-analyzer");

pub struct RustLspAdapter;
