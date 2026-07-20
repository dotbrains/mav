use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use language::Toolchain;
use lsp::{DiagnosticServerCapabilities, LanguageServerId, LanguageServerName};
use util::rel_path::RelPath;
use worktree::WorktreeId;

use crate::project_settings::BinarySettings;

#[derive(Clone)]
pub(crate) struct UnifiedLanguageServer {
    pub(crate) id: LanguageServerId,
    pub(crate) project_roots: HashSet<Arc<RelPath>>,
}

/// Settings that affect language server identity.
///
/// Dynamic settings (`LspSettings::settings`) are excluded because they can be
/// updated via `workspace/didChangeConfiguration` without restarting the server.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct LanguageServerSeedSettings {
    pub(crate) binary: Option<BinarySettings>,
    pub(crate) initialization_options: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct LanguageServerSeed {
    pub(crate) worktree_id: WorktreeId,
    pub(crate) name: LanguageServerName,
    pub(crate) toolchain: Option<Toolchain>,
    pub(crate) settings: LanguageServerSeedSettings,
}

#[derive(Default, Debug)]
pub(crate) struct DynamicRegistrations {
    pub(crate) did_change_watched_files: HashSet<String>,
    pub(crate) diagnostics: HashMap<Option<String>, DiagnosticServerCapabilities>,
}
