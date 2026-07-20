use anyhow::{Context as _, Result};
use client::{TypedEnvelope, proto};
use gpui::{AsyncApp, Context, Entity, SharedString, TaskExt as _};
use language::{Buffer, LanguageServerId, LanguageServerName};
use lsp::LanguageServerSelector;
use text::BufferId;
use util::ResultExt as _;

use super::{LspStore, ProgressToken};

impl LspStore {
    pub async fn handle_restart_language_servers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RestartLanguageServers>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        this.update(&mut cx, |lsp_store, cx| {
            let buffers =
                lsp_store.buffer_ids_to_buffers(envelope.payload.buffer_ids.into_iter(), cx);
            lsp_store.restart_language_servers_for_buffers(
                buffers,
                envelope
                    .payload
                    .only_servers
                    .into_iter()
                    .filter_map(language_server_selector_from_proto)
                    .collect(),
                true,
                cx,
            );
        });

        Ok(proto::Ack {})
    }

    pub async fn handle_stop_language_servers(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::StopLanguageServers>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        lsp_store.update(&mut cx, |lsp_store, cx| {
            if envelope.payload.all
                && envelope.payload.also_servers.is_empty()
                && envelope.payload.buffer_ids.is_empty()
            {
                lsp_store.stop_all_language_servers(cx);
            } else {
                let buffers =
                    lsp_store.buffer_ids_to_buffers(envelope.payload.buffer_ids.into_iter(), cx);
                lsp_store
                    .stop_language_servers_for_buffers(
                        buffers,
                        envelope
                            .payload
                            .also_servers
                            .into_iter()
                            .filter_map(language_server_selector_from_proto)
                            .collect(),
                        cx,
                    )
                    .detach_and_log_err(cx);
            }
        });

        Ok(proto::Ack {})
    }

    pub async fn handle_cancel_language_server_work(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::CancelLanguageServerWork>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        lsp_store.update(&mut cx, |lsp_store, cx| {
            if let Some(work) = envelope.payload.work {
                match work {
                    proto::cancel_language_server_work::Work::Buffers(buffers) => {
                        let buffers =
                            lsp_store.buffer_ids_to_buffers(buffers.buffer_ids.into_iter(), cx);
                        lsp_store.cancel_language_server_work_for_buffers(buffers, cx);
                    }
                    proto::cancel_language_server_work::Work::LanguageServerWork(work) => {
                        let server_id = LanguageServerId::from_proto(work.language_server_id);
                        let token = work
                            .token
                            .map(|token| {
                                ProgressToken::from_proto(token)
                                    .context("invalid work progress token")
                            })
                            .transpose()?;
                        lsp_store.cancel_language_server_work(server_id, token, cx);
                    }
                }
            }
            anyhow::Ok(())
        })?;

        Ok(proto::Ack {})
    }

    fn buffer_ids_to_buffers(
        &mut self,
        buffer_ids: impl Iterator<Item = u64>,
        cx: &mut Context<Self>,
    ) -> Vec<Entity<Buffer>> {
        buffer_ids
            .into_iter()
            .flat_map(|buffer_id| {
                self.buffer_store
                    .read(cx)
                    .get(BufferId::new(buffer_id).log_err()?)
            })
            .collect::<Vec<_>>()
    }
}

fn language_server_selector_from_proto(
    selector: proto::LanguageServerSelector,
) -> Option<LanguageServerSelector> {
    Some(match selector.selector? {
        proto::language_server_selector::Selector::ServerId(server_id) => {
            LanguageServerSelector::Id(LanguageServerId::from_proto(server_id))
        }
        proto::language_server_selector::Selector::Name(name) => {
            LanguageServerSelector::Name(LanguageServerName(SharedString::from(name)))
        }
    })
}
