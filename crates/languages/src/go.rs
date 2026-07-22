use anyhow::{Context as _, Result};
use async_trait::async_trait;
use collections::HashMap;
use futures::StreamExt;
use gpui::{App, AsyncApp, Entity, Task};
use http_client::github::latest_github_release;
pub use language::*;
use language::{
    LanguageName, LanguageToolchainStore, LspAdapterDelegate, LspInstaller,
    language_settings::LanguageSettings,
};
use lsp::{LanguageServerBinary, LanguageServerName};

use project::lsp_store::language_server_settings;
use regex::Regex;
use serde_json::{Value, json};
use settings::SemanticTokenRules;
use smol::fs;
use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    future::Future,
    ops::Range,
    path::{Path, PathBuf},
    process::Output,
    str,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};
use task::{TaskTemplate, TaskTemplates, TaskVariables, VariableName};
use util::{ResultExt, fs::remove_matching, maybe, merge_json_value_into};

pub(crate) fn semantic_token_rules() -> SemanticTokenRules {
    let content = grammars::get_file("go/semantic_token_rules.json")
        .expect("missing go/semantic_token_rules.json");
    let json = std::str::from_utf8(&content.data).expect("invalid utf-8 in semantic_token_rules");
    settings::parse_json_with_comments::<SemanticTokenRules>(json)
        .expect("failed to parse go semantic_token_rules.json")
}

mod context;
mod lsp_adapter;
mod test_tasks;

pub(crate) use self::{context::GoContextProvider, lsp_adapter::GoLspAdapter};
use self::{
    context::adjust_runs,
    lsp_adapter::{GO_ESCAPE_SUBTEST_NAME_REGEX, VERSION_REGEX, server_binary_arguments},
    test_tasks::{get_cached_server_binary, go_test_task_template, parse_version_output},
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::go::context::extract_subtest_name;
    use crate::language;
    use gpui::{AppContext, Hsla, TestAppContext};
    use task::TaskContext;
    use theme::SyntaxTheme;

    fn go_language() -> Arc<Language> {
        let language = language("go", tree_sitter_go::LANGUAGE.into());
        Arc::new(
            Arc::try_unwrap(language)
                .unwrap()
                .with_context_provider(Some(Arc::new(GoContextProvider))),
        )
    }

    mod basic_runnables;
    mod field_resolution;
    mod labels;
    mod stress;
    mod subtests;
    mod table_maps;
    mod table_slices;
    mod task_templates;
}
