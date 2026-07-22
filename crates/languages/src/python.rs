use anyhow::Result;
use anyhow::{Context as _, ensure};
use async_trait::async_trait;
use collections::HashMap;
use futures::future::BoxFuture;
use futures::lock::OwnedMutexGuard;
use futures::{AsyncBufReadExt, StreamExt as _};
use gpui::{App, AsyncApp, Entity, SharedString, Task};
use http_client::github::{AssetKind, GitHubLspBinaryVersion, latest_github_release};
use language::language_settings::LanguageSettings;
use language::{
    Buffer, ContextLocation, DynLspInstaller, LanguageToolchainStore, LspInstaller, Symbol,
};
use language::{ContextProvider, LspAdapter, LspAdapterDelegate};
use language::{LanguageName, ManifestName, ManifestProvider, ManifestQuery};
use language::{Toolchain, ToolchainList, ToolchainLister, ToolchainMetadata};
use lsp::{CompletionItemKind, LanguageServerBinary, Uri};
use lsp::{LanguageServerBinaryOptions, LanguageServerName};
use node_runtime::{NodeRuntime, VersionStrategy};
use pet_core::Configuration;
use pet_core::os_environment::Environment;
use pet_core::python_environment::{PythonEnvironment, PythonEnvironmentKind};
use pet_virtualenv::is_virtualenv_dir;
use project::Fs;
use project::lsp_store::language_server_settings;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use settings::{SemanticTokenRules, Settings};
use terminal::terminal_settings::TerminalSettings;

use smol::lock::OnceCell;
use std::cmp::{Ordering, Reverse};
use std::env::consts;
use util::command::Stdio;

use util::command::new_command;
use util::fs::{make_file_executable, remove_matching};
use util::paths::PathStyle;
use util::rel_path::RelPath;

use http_client::github_download::{GithubBinaryMetadata, download_server_binary};
use parking_lot::Mutex;
use std::str::FromStr;
use std::{
    borrow::Cow,
    fmt::Write,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};
use task::{ShellKind, TaskTemplate, TaskTemplates, VariableName};
use util::{ResultExt, maybe};

pub(crate) fn semantic_token_rules() -> SemanticTokenRules {
    let content = grammars::get_file("python/semantic_token_rules.json")
        .expect("missing python/semantic_token_rules.json");
    let json = std::str::from_utf8(&content.data).expect("invalid utf-8 in semantic_token_rules");
    settings::parse_json_with_comments::<SemanticTokenRules>(json)
        .expect("failed to parse python semantic_token_rules.json")
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PythonToolchainData {
    #[serde(flatten)]
    environment: PythonEnvironment,
    #[serde(skip_serializing_if = "Option::is_none")]
    activation_scripts: Option<HashMap<ShellKind, PathBuf>>,
}

pub(crate) struct PyprojectTomlManifestProvider;

impl ManifestProvider for PyprojectTomlManifestProvider {
    fn name(&self) -> ManifestName {
        SharedString::new_static("pyproject.toml").into()
    }

    fn search(
        &self,
        ManifestQuery {
            path,
            depth,
            delegate,
        }: ManifestQuery,
    ) -> Option<Arc<RelPath>> {
        const WORKSPACE_LOCKFILES: &[&str] =
            &["uv.lock", "poetry.lock", "pdm.lock", "Pipfile.lock"];

        let mut innermost_pyproject = None;
        let mut outermost_workspace_root = None;

        for path in path.ancestors().take(depth) {
            let pyproject_path = path.join(RelPath::unix("pyproject.toml").unwrap());
            if delegate.exists(&pyproject_path, Some(false)) {
                if innermost_pyproject.is_none() {
                    innermost_pyproject = Some(Arc::from(path));
                }

                let has_lockfile = WORKSPACE_LOCKFILES.iter().any(|lockfile| {
                    let lockfile_path = path.join(RelPath::unix(lockfile).unwrap());
                    delegate.exists(&lockfile_path, Some(false))
                });
                if has_lockfile {
                    outermost_workspace_root = Some(Arc::from(path));
                }
            }
        }

        outermost_workspace_root.or(innermost_pyproject)
    }
}

enum TestRunner {
    UNITTEST,
    PYTEST,
}

impl FromStr for TestRunner {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "unittest" => Ok(Self::UNITTEST),
            "pytest" => Ok(Self::PYTEST),
            _ => Err(()),
        }
    }
}

/// Pyright assigns each completion item a `sortText` of the form `XX.YYYY.name`.
/// Where `XX` is the sorting category, `YYYY` is based on most recent usage,
/// and `name` is the symbol name itself.
///
/// The problem with it is that Pyright adjusts the sort text based on previous resolutions (items for which we've issued `completion/resolve` call have their sortText adjusted),
/// which - long story short - makes completion items list non-stable. Pyright probably relies on VSCode's implementation detail.
/// see https://github.com/microsoft/pyright/blob/95ef4e103b9b2f129c9320427e51b73ea7cf78bd/packages/pyright-internal/src/languageService/completionProvider.ts#LL2873
///
/// upd 02.12.25:
/// Decided to ignore Pyright's sortText() completely and to manually sort all entries
fn process_pyright_completions(items: &mut [lsp::CompletionItem]) {
    for item in items {
        let is_named_argument = item.label.ends_with('=');

        let is_dunder = item.label.starts_with("__") && item.label.ends_with("__");

        let visibility_priority = if is_dunder {
            '3'
        } else if item.label.starts_with("__") {
            '2' // private non-dunder
        } else if item.label.starts_with('_') {
            '1' // protected
        } else {
            '0' // public
        };

        let is_external = item
            .detail
            .as_ref()
            .is_some_and(|detail| detail == "Auto-import");

        let source_priority = if is_external { '1' } else { '0' };

        // Kind priority within same visibility level
        let kind_priority = match item.kind {
            Some(lsp::CompletionItemKind::KEYWORD) => '0',
            Some(lsp::CompletionItemKind::ENUM_MEMBER) => '1',
            Some(lsp::CompletionItemKind::FIELD) => '2',
            Some(lsp::CompletionItemKind::PROPERTY) => '3',
            Some(lsp::CompletionItemKind::VARIABLE) => '4',
            Some(lsp::CompletionItemKind::CONSTANT) => '5',
            Some(lsp::CompletionItemKind::METHOD) => '6',
            Some(lsp::CompletionItemKind::FUNCTION) => '6',
            Some(lsp::CompletionItemKind::CLASS) => '7',
            Some(lsp::CompletionItemKind::MODULE) => '8',

            _ => 'z',
        };

        // Named arguments get higher priority
        let argument_priority = if is_named_argument { '0' } else { '1' };

        item.sort_text = Some(format!(
            "{}{}{}{}{}",
            argument_priority, source_priority, visibility_priority, kind_priority, item.label
        ));
    }
}

fn label_for_pyright_completion(
    item: &lsp::CompletionItem,
    language: &Arc<language::Language>,
) -> Option<language::CodeLabel> {
    let label = &item.label;
    let label_len = label.len();
    let grammar = language.grammar()?;
    let highlight_id = highlight_id_for_completion(item.kind?, grammar)?;

    let mut text = label.clone();
    if let Some(completion_details) = item
        .label_details
        .as_ref()
        .and_then(|details| details.description.as_ref())
    {
        write!(&mut text, " {}", completion_details).ok();
    }
    Some(language::CodeLabel::filtered(
        text,
        label_len,
        item.filter_text.as_deref(),
        highlight_id
            .map(|id| (0..label_len, id))
            .into_iter()
            .collect(),
    ))
}

fn label_for_python_symbol(
    symbol: &Symbol,
    language: &Arc<language::Language>,
) -> Option<language::CodeLabel> {
    let name = &symbol.name;
    let (text, filter_range, display_range) = match symbol.kind {
        lsp::SymbolKind::METHOD | lsp::SymbolKind::FUNCTION => {
            let text = format!("def {}():\n", name);
            let filter_range = 4..4 + name.len();
            let display_range = 0..filter_range.end;
            (text, filter_range, display_range)
        }
        lsp::SymbolKind::CLASS => {
            let text = format!("class {}:", name);
            let filter_range = 6..6 + name.len();
            let display_range = 0..filter_range.end;
            (text, filter_range, display_range)
        }
        lsp::SymbolKind::CONSTANT => {
            let text = format!("{} = 0", name);
            let filter_range = 0..name.len();
            let display_range = 0..filter_range.end;
            (text, filter_range, display_range)
        }
        _ => return None,
    };
    Some(language::CodeLabel::new(
        text[display_range.clone()].to_string(),
        filter_range,
        language.highlight_text(&text.as_str().into(), display_range),
    ))
}

/// Returns the highlight ID for the given completion item kind, if it is supported.
///
/// The outer `Option` is `None` if the item kind returned by the language server is not covered.
/// The inner `Option` is `None` if the item kind is covered, but the highlight name is not present in the grammar.
fn highlight_id_for_completion(
    item_kind: CompletionItemKind,
    grammar: &Arc<language::Grammar>,
) -> Option<Option<language::HighlightId>> {
    match item_kind {
        CompletionItemKind::METHOD => Some(grammar.highlight_id_for_name("function.method.call")),
        CompletionItemKind::FUNCTION => Some(grammar.highlight_id_for_name("function.call")),
        CompletionItemKind::CLASS => Some(grammar.highlight_id_for_name("type")),
        CompletionItemKind::CONSTANT => Some(grammar.highlight_id_for_name("constant")),
        CompletionItemKind::VARIABLE => Some(grammar.highlight_id_for_name("variable")),
        _ => None,
    }
}

mod based_pyright;
mod context;
mod pylsp;
mod pyright;
mod ruff;
#[cfg(test)]
mod tests;
mod toolchain;
mod ty;

pub(crate) use based_pyright::BasedPyrightLspAdapter;
pub(crate) use context::PythonContextProvider;
pub(crate) use pylsp::PyLspAdapter;
pub(crate) use pyright::PyrightLspAdapter;
pub(crate) use ruff::RuffLspAdapter;
pub(crate) use toolchain::PythonToolchainProvider;
pub use ty::TyLspAdapter;
