use std::{ops::Range, path::PathBuf};

use collections::HashMap;
use gpui::Entity;
use language::Buffer;
use rpc::AnyProtoClient;
use text::Anchor;

use crate::lsp_store::LocalLspStore;

#[derive(Debug)]
pub struct FormattableBuffer {
    pub(crate) handle: Entity<Buffer>,
    pub(crate) abs_path: Option<PathBuf>,
    pub(crate) env: Option<HashMap<String, String>>,
    pub(crate) ranges: Option<Vec<Range<Anchor>>>,
}

pub struct RemoteLspStore {
    pub(crate) upstream_client: Option<AnyProtoClient>,
    pub(crate) upstream_project_id: u64,
}

pub(crate) enum LspStoreMode {
    Local(LocalLspStore),   // ssh host and collab host
    Remote(RemoteLspStore), // collab guest
}

impl LspStoreMode {
    pub(crate) fn is_local(&self) -> bool {
        matches!(self, LspStoreMode::Local(_))
    }
}
