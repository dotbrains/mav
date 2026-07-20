use std::{borrow::Cow, path::PathBuf};

use gpui::SharedString;
use language::{DiagnosticEntry, PointUtf16, Unclipped};
use lsp::LanguageServerId;

#[derive(Debug)]
pub struct DocumentDiagnosticsUpdate<'a, D> {
    pub diagnostics: D,
    pub result_id: Option<SharedString>,
    pub registration_id: Option<SharedString>,
    pub server_id: LanguageServerId,
    pub disk_based_sources: Cow<'a, [String]>,
}

pub struct DocumentDiagnostics {
    pub(crate) diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
    pub(crate) document_abs_path: PathBuf,
    pub(crate) version: Option<i32>,
}
