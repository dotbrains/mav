use super::{LspStore, LspStoreEvent};
use crate::proto::{self, update_language_server};
use gpui::Context;
use lsp::LanguageServer;

pub(super) fn notify_server_capabilities_updated(
    server: &LanguageServer,
    cx: &mut Context<LspStore>,
) {
    if let Some(capabilities) = serde_json::to_string(&server.capabilities()).ok() {
        cx.emit(LspStoreEvent::LanguageServerUpdate {
            language_server_id: server.server_id(),
            name: Some(server.name()),
            message: update_language_server::Variant::MetadataUpdated(
                proto::ServerMetadataUpdated {
                    capabilities: Some(capabilities),
                    binary: Some(proto::LanguageServerBinaryInfo {
                        path: server.binary().path.to_string_lossy().into_owned(),
                        arguments: server
                            .binary()
                            .arguments
                            .iter()
                            .map(|arg| arg.to_string_lossy().into_owned())
                            .collect(),
                    }),
                    configuration: serde_json::to_string(server.configuration()).ok(),
                    workspace_folders: server
                        .workspace_folders()
                        .iter()
                        .map(|uri| uri.to_string())
                        .collect(),
                },
            ),
        });
    }
}
