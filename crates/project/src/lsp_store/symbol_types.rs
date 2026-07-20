use std::{ops::Range, path::Path, sync::Arc};

use language::{PointUtf16, Unclipped};
use lsp::{LanguageServerId, LanguageServerName};

use crate::{ProjectPath, WorktreeId};

#[derive(Clone, Debug)]
pub(crate) struct CoreSymbol {
    pub(crate) language_server_name: LanguageServerName,
    pub(crate) source_worktree_id: WorktreeId,
    pub(crate) source_language_server_id: LanguageServerId,
    pub(crate) path: SymbolLocation,
    pub(crate) name: String,
    pub(crate) kind: lsp::SymbolKind,
    pub(crate) range: Range<Unclipped<PointUtf16>>,
    pub(crate) container_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolLocation {
    InProject(ProjectPath),
    OutsideProject {
        abs_path: Arc<Path>,
        signature: [u8; 32],
    },
}

impl SymbolLocation {
    pub(crate) fn file_name(&self) -> Option<&str> {
        match self {
            Self::InProject(path) => path.path.file_name(),
            Self::OutsideProject { abs_path, .. } => abs_path.file_name()?.to_str(),
        }
    }
}
