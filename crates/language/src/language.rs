//! The `language` crate provides a large chunk of Mav's language-related
//! features (the other big contributors being project and lsp crates that revolve around LSP features).
//! Namely, this crate:
//! - Provides [`Language`], [`Grammar`] and [`LanguageRegistry`] types that
//!   use Tree-sitter to provide syntax highlighting to the editor; note though that `language` doesn't perform the highlighting by itself. It only maps ranges in a buffer to colors. Treesitter is also used for buffer outlines (lists of symbols in a buffer)
//! - Exposes [`LanguageConfig`] that describes how constructs (like brackets or line comments) should be handled by the editor for a source file of a particular language.
//!
//! Notably we do *not* assign a single language to a single file; in real world a single file can consist of multiple programming languages - HTML is a good example of that - and `language` crate tends to reflect that status quo in its API.
mod buffer;
mod cached_lsp_adapter;
mod code_label;
mod diagnostic;
mod diagnostic_set;
#[cfg(any(test, feature = "test-support"))]
mod fake_lsp_adapter;
mod file_content;
mod language_registry;
mod language_runtime;
mod language_scope;
#[cfg(test)]
mod language_tests;
mod lsp_adapter;
mod lsp_conversions;
#[cfg(any(test, feature = "test-support"))]
mod test_languages;

pub mod language_settings;
mod manifest;
pub mod modeline;
mod outline;
pub mod proto;
mod runnable;
mod syntax_map;
mod task_context;
mod text_diff;
mod toolchain;

#[cfg(test)]
pub mod buffer_tests;

pub use crate::language_settings::{
    AutoIndentMode, EditPredictionPromptFormat, EditPredictionsMode, IndentGuideSettings,
    ZetaVersion,
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use collections::{HashMap, HashSet};
use futures::Future;
use futures::future::LocalBoxFuture;
use futures::lock::OwnedMutexGuard;
use gpui::{App, AsyncApp, Entity};
use http_client::HttpClient;

pub use language_core::highlight_map::{HighlightId, HighlightMap};

use futures::future::FutureExt as _;
pub use language_core::{
    BlockCommentConfig, BracketPair, BracketPairConfig, BracketPairContent, BracketsConfig,
    BracketsPatternConfig, CodeLabel, CodeLabelBuilder, DebugVariablesConfig, DebuggerTextObject,
    DecreaseIndentConfig, Grammar, GrammarId, HighlightsConfig, IndentConfig, InjectionConfig,
    InjectionPatternConfig, JsxTagAutoCloseConfig, LanguageConfig, LanguageConfigOverride,
    LanguageId, LanguageMatcher, OrderedListConfig, OutlineConfig, Override, OverrideConfig,
    OverrideEntry, PromptResponseContext, RedactionConfig, RunnableCapture, RunnableConfig,
    SoftWrap, Symbol, TaskListConfig, TextObject, TextObjectConfig, ToLspPosition,
    WrapCharactersConfig, auto_indent_using_last_non_empty_line_default, deserialize_regex,
    deserialize_regex_vec, regex_json_schema, regex_vec_json_schema, serialize_regex,
};
pub use language_registry::{
    LanguageName, LanguageServerStatusUpdate, LoadedLanguage, ServerHealth,
};
use lsp::{
    CodeActionKind, InitializeParams, LanguageServerBinary, LanguageServerBinaryOptions, Uri,
};
pub use manifest::{ManifestDelegate, ManifestName, ManifestProvider, ManifestQuery};
pub use modeline::{ModelineSettings, parse_modeline};
use parking_lot::Mutex;
use regex::Regex;
pub use runnable::{ResolvedRunnable, RunnableMatchCapture, RunnableRange, RunnableResolver};
use semver::Version;
use serde_json::Value;
use settings::WorktreeId;
use std::{
    ffi::OsStr,
    fmt::Debug,
    hash::Hash,
    mem,
    ops::{DerefMut, Range},
    path::{Path, PathBuf},
    str,
    sync::{Arc, LazyLock},
};
use syntax_map::{QueryCursorHandle, SyntaxSnapshot};
use task::RunnableTag;
pub use task_context::{ContextLocation, ContextProvider};
pub use text_diff::{
    DiffOptions, apply_diff_patch, apply_reversed_diff_patch, char_diff, line_diff, text_diff,
    text_diff_with_options, unified_diff, unified_diff_with_context, unified_diff_with_offsets,
    word_diff_ranges,
};
use theme::SyntaxTheme;
pub use toolchain::{
    LanguageToolchainStore, LocalLanguageToolchainStore, Toolchain, ToolchainList, ToolchainLister,
    ToolchainMetadata, ToolchainScope,
};
use tree_sitter::{self, QueryCursor, WasmStore, wasmtime};
use util::rel_path::RelPath;

pub use buffer::Operation;
pub use buffer::*;
pub use diagnostic::{Diagnostic, DiagnosticSourceKind};
pub use diagnostic_set::{DiagnosticEntry, DiagnosticEntryRef, DiagnosticGroup};
pub use file_content::{ByteContent, FILE_ANALYSIS_BYTES, analyze_byte_content};
pub use language_registry::{
    AvailableLanguage, BinaryStatus, LanguageNotFound, LanguageQueries, LanguageRegistry,
    QUERY_FILENAME_PREFIXES,
};
pub use lsp::{LanguageServerId, LanguageServerName};
pub use outline::*;
pub use syntax_map::{
    OwnedSyntaxLayer, SyntaxLayer, SyntaxMapMatches, ToTreeSitterPoint, TreeSitterOptions,
};
pub use text::{AnchorRangeExt, LineEnding};
pub use tree_sitter::{Node, Parser, QueryCapture, Tree, TreeCursor};

pub(crate) fn to_settings_soft_wrap(value: language_core::SoftWrap) -> settings::SoftWrap {
    match value {
        language_core::SoftWrap::None => settings::SoftWrap::None,
        language_core::SoftWrap::PreferLine => settings::SoftWrap::PreferLine,
        language_core::SoftWrap::EditorWidth => settings::SoftWrap::EditorWidth,
        language_core::SoftWrap::Bounded => settings::SoftWrap::Bounded,
    }
}

static QUERY_CURSORS: Mutex<Vec<QueryCursor>> = Mutex::new(vec![]);
static PARSERS: Mutex<Vec<Parser>> = Mutex::new(vec![]);
pub use cached_lsp_adapter::{CachedLspAdapter, LanguageServerBinaryLocations};
pub use code_label::CodeLabelExt;
#[cfg(any(test, feature = "test-support"))]
pub use fake_lsp_adapter::FakeLspAdapter;
pub use language_runtime::{Language, build_highlight_map};
pub use language_scope::LanguageScope;
pub use lsp_adapter::{DynLspInstaller, LspAdapter, LspAdapterDelegate, LspInstaller};
pub use lsp_conversions::{point_from_lsp, point_to_lsp, range_from_lsp, range_to_lsp};
#[cfg(any(test, feature = "test-support"))]
pub use test_languages::{markdown_lang, rust_lang};
pub fn with_parser<F, R>(func: F) -> R
where
    F: FnOnce(&mut Parser) -> R,
{
    let mut parser = PARSERS.lock().pop().unwrap_or_else(|| {
        let mut parser = Parser::new();
        parser
            .set_wasm_store(WasmStore::new(&WASM_ENGINE).unwrap())
            .unwrap();
        parser
    });
    // Tree-sitter auto-resets the parser at the end of a successful parse,
    // but the cancellation paths (progress callback returning `Break`,
    // cancelled balancing) leave outstanding state on the parser. The next
    // call to `parse_with_options` would then *resume* that cancelled parse
    // instead of starting fresh.
    parser.reset();
    parser.set_included_ranges(&[]).unwrap();
    let result = func(&mut parser);
    PARSERS.lock().push(parser);
    result
}

pub fn with_query_cursor<F, R>(func: F) -> R
where
    F: FnOnce(&mut QueryCursor) -> R,
{
    let mut cursor = QueryCursorHandle::new();
    func(cursor.deref_mut())
}

static WASM_ENGINE: LazyLock<wasmtime::Engine> = LazyLock::new(|| {
    wasmtime::Engine::new(&wasmtime::Config::new()).expect("Failed to create Wasmtime engine")
});

/// A shared grammar for plain text, exposed for reuse by downstream crates.
pub static PLAIN_TEXT: LazyLock<Arc<Language>> = LazyLock::new(|| {
    Arc::new(Language::new(
        LanguageConfig {
            name: "Plain Text".into(),
            soft_wrap: Some(SoftWrap::EditorWidth),
            matcher: LanguageMatcher {
                path_suffixes: vec!["txt".to_owned()],
                first_line_pattern: None,
                modeline_aliases: vec!["text".to_owned(), "txt".to_owned()],
            },
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "\"".to_string(),
                        end: "\"".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "'".to_string(),
                        end: "'".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                ],
                disabled_scopes_by_bracket_ix: Default::default(),
            },
            ..Default::default()
        },
        None,
    ))
});

/// Commands that the client (editor) handles locally rather than forwarding
/// to the language server. Servers embed these in code lens and code action
/// responses when they want the editor to perform a well-known UI action.
#[derive(Debug, Clone)]
pub enum ClientCommand {
    /// Open a location list (references panel / peek view).
    ShowLocations,
    /// Schedule a task from an LSP command's arguments.
    ScheduleTask(task::TaskTemplate),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Location {
    pub buffer: Entity<Buffer>,
    pub range: Range<Anchor>,
}
