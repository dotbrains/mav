use client::proto;
use futures::StreamExt;
use gpui::{Context, Task};
use language::{BinaryStatus, LanguageRegistry};
use lsp::LanguageServerId;
use std::sync::Arc;

use super::{LspStore, LspStoreEvent};

pub(super) fn subscribe_to_binary_statuses(
    languages: &Arc<LanguageRegistry>,
    cx: &mut Context<'_, LspStore>,
) -> Task<()> {
    let mut server_statuses = languages.language_server_binary_statuses();
    cx.spawn(async move |lsp_store, cx| {
        while let Some((server_name, binary_status)) = server_statuses.next().await {
            if lsp_store
                .update(cx, |_, cx| {
                    let mut message = None;
                    let binary_status = match binary_status {
                        BinaryStatus::None => proto::ServerBinaryStatus::None,
                        BinaryStatus::CheckingForUpdate => {
                            proto::ServerBinaryStatus::CheckingForUpdate
                        }
                        BinaryStatus::Downloading => proto::ServerBinaryStatus::Downloading,
                        BinaryStatus::Starting => proto::ServerBinaryStatus::Starting,
                        BinaryStatus::Stopping => proto::ServerBinaryStatus::Stopping,
                        BinaryStatus::Stopped => proto::ServerBinaryStatus::Stopped,
                        BinaryStatus::Failed { error } => {
                            message = Some(error);
                            proto::ServerBinaryStatus::Failed
                        }
                    };
                    cx.emit(LspStoreEvent::LanguageServerUpdate {
                        language_server_id: LanguageServerId(0),
                        name: Some(server_name),
                        message: proto::update_language_server::Variant::StatusUpdate(
                            proto::StatusUpdate {
                                message,
                                status: Some(proto::status_update::Status::Binary(
                                    binary_status as i32,
                                )),
                            },
                        ),
                    });
                })
                .is_err()
            {
                break;
            }
        }
    })
}
