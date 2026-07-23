use crate::TestServer;
use call::ActiveCall;
use collections::{HashMap, HashSet};

use dap::{Capabilities, adapters::DebugTaskDefinition, transport::RequestHandling};
use debugger_ui::debugger_panel::DebugPanel;
use editor::{Editor, EditorMode, LSP_REQUEST_DEBOUNCE_TIMEOUT, MultiBuffer};
use extension::ExtensionHostProxy;
use fs::{FakeFs, Fs as _, RemoveOptions};
use futures::StreamExt as _;
use gpui::{
    AppContext as _, BackgroundExecutor, TestAppContext, UpdateGlobal as _, VisualContext as _,
};
use http_client::BlockedHttpClient;
use language::{
    FakeLspAdapter, Language, LanguageConfig, LanguageMatcher, LanguageRegistry,
    language_settings::{Formatter, FormatterList, LanguageSettings},
    rust_lang, tree_sitter_typescript,
};
use node_runtime::NodeRuntime;
use project::{
    ProjectPath,
    debugger::session::ThreadId,
    lsp_store::{
        FormatTrigger, LspFormatTarget,
        log_store::{self, GlobalLogStore},
    },
    trusted_worktrees::{PathTrust, TrustedWorktrees},
};
use remote::RemoteClient;
use remote_server::{HeadlessAppState, HeadlessProject};
use rpc::proto;
use serde_json::json;
use settings::{
    InlayHintSettingsContent, LanguageServerFormatterSpecifier, PrettierSettingsContent,
    SettingsStore,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use task::TcpArgumentsTemplate;
use util::{path, rel_path::rel_path};

mod debugger;
mod document_links;
mod formatting;
mod git_branches;
mod git_worktrees;
mod language_server_restart;
mod sharing_remote_project;
mod slow_adapter_startup;
mod worktree_trust;
