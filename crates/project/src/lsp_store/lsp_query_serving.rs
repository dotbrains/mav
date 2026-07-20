use anyhow::Result;
use clock::Global;
use collections::HashMap;
use gpui::{AppContext as _, AsyncApp, Context, Entity, Task};
use language::Buffer;
use rpc::proto::{self, LspRequestId, LspRequestMessage as _};
use std::{any::TypeId, ops::Range};
use text::{Anchor, BufferId, OffsetRangeExt as _};

use super::{LspKey, LspStore};
use crate::{LanguageServerToQuery, lsp_command::LspCommand};

impl LspStore {
    pub(super) async fn deduplicate_range_based_lsp_requests<T>(
        lsp_store: &Entity<Self>,
        server_id: Option<lsp::LanguageServerId>,
        lsp_request_id: LspRequestId,
        proto_request: &T::ProtoRequest,
        range: Range<Anchor>,
        cx: &mut AsyncApp,
    ) -> Result<()>
    where
        T: LspCommand,
        T::ProtoRequest: proto::LspRequestMessage,
    {
        let buffer_id = BufferId::new(proto_request.buffer_id())?;
        let version = language::proto::deserialize_version(proto_request.buffer_version());
        let buffer = lsp_store.update(cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        buffer
            .update(cx, |buffer, _| buffer.wait_for_version(version))
            .await?;
        lsp_store.update(cx, |lsp_store, cx| {
            let buffer_snapshot = buffer.read(cx).snapshot();
            let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
            let chunks_queried_for = lsp_data
                .inlay_hints
                .applicable_chunks(&[range.to_point(&buffer_snapshot)])
                .collect::<Vec<_>>();
            match chunks_queried_for.as_slice() {
                &[chunk] => {
                    let key = LspKey {
                        request_type: TypeId::of::<T>(),
                        server_queried: server_id,
                    };
                    let previous_request = lsp_data
                        .chunk_lsp_requests
                        .entry(key)
                        .or_default()
                        .insert(chunk, lsp_request_id);
                    if let Some((previous_request, running_requests)) =
                        previous_request.zip(lsp_data.lsp_requests.get_mut(&key))
                    {
                        running_requests.remove(&previous_request);
                    }
                }
                _ambiguous_chunks => {
                    // The queried range does not map to one chunk, so let the query run and rely on buffer-version checks.
                }
            }
            anyhow::Ok(())
        })?;

        Ok(())
    }

    pub(super) async fn query_lsp_locally<T>(
        lsp_store: Entity<Self>,
        for_server_id: Option<lsp::LanguageServerId>,
        sender_id: proto::PeerId,
        lsp_request_id: LspRequestId,
        proto_request: T::ProtoRequest,
        position: Option<Anchor>,
        cx: &mut AsyncApp,
    ) -> Result<()>
    where
        T: LspCommand + Clone,
        T::ProtoRequest: proto::LspRequestMessage,
        <T::ProtoRequest as proto::RequestMessage>::Response:
            Into<<T::ProtoRequest as proto::LspRequestMessage>::Response>,
    {
        let (buffer_version, buffer) =
            Self::wait_for_buffer_version::<T>(&lsp_store, &proto_request, cx).await?;
        let request =
            T::from_proto(proto_request, lsp_store.clone(), buffer.clone(), cx.clone()).await?;
        let key = LspKey {
            request_type: TypeId::of::<T>(),
            server_queried: for_server_id,
        };
        lsp_store.update(cx, |lsp_store, cx| {
            let request_task = match for_server_id {
                Some(server_id) => {
                    let server_task = lsp_store.request_lsp(
                        buffer.clone(),
                        LanguageServerToQuery::Other(server_id),
                        request.clone(),
                        cx,
                    );
                    cx.background_spawn(async move {
                        let mut responses = Vec::new();
                        match server_task.await {
                            Ok(response) => responses.push((server_id, response)),
                            // rust-analyzer likes to error with this when its still loading up
                            Err(e) if format!("{e:#}").ends_with("content modified") => (),
                            Err(e) => log::error!(
                                "Error handling response for request {request:?}: {e:#}"
                            ),
                        }
                        responses
                    })
                }
                None => lsp_store.request_multiple_lsp_locally(&buffer, position, request, cx),
            };
            let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
            if T::ProtoRequest::stop_previous_requests() {
                if let Some(lsp_requests) = lsp_data.lsp_requests.get_mut(&key) {
                    lsp_requests.clear();
                }
            }
            lsp_data.lsp_requests.entry(key).or_default().insert(
                lsp_request_id,
                cx.spawn(async move |lsp_store, cx| {
                    let response = request_task.await;
                    lsp_store
                        .update(cx, |lsp_store, cx| {
                            if let Some((client, project_id)) = lsp_store.downstream_client.clone()
                            {
                                let response = response
                                    .into_iter()
                                    .map(|(server_id, response)| {
                                        (
                                            server_id.to_proto(),
                                            T::response_to_proto(
                                                response,
                                                lsp_store,
                                                sender_id,
                                                &buffer_version,
                                                cx,
                                            )
                                            .into(),
                                        )
                                    })
                                    .collect::<HashMap<_, _>>();
                                if let Err(e) = client.send_lsp_response::<T::ProtoRequest>(
                                    project_id,
                                    lsp_request_id,
                                    response,
                                ) {
                                    log::error!("Failed to send LSP response: {e:#}");
                                }
                            }
                        })
                        .ok();
                }),
            );
        });
        Ok(())
    }

    pub(super) fn serve_lsp_query<T>(
        &mut self,
        server_id: Option<lsp::LanguageServerId>,
        sender_id: proto::PeerId,
        lsp_request_id: LspRequestId,
        buffer: &Entity<Buffer>,
        buffer_version: Global,
        fetch_task: Task<HashMap<lsp::LanguageServerId, T::Response>>,
        cx: &mut Context<Self>,
    ) where
        T: LspCommand + 'static,
        T::ProtoRequest: proto::LspRequestMessage,
        <T::ProtoRequest as proto::RequestMessage>::Response:
            Into<<T::ProtoRequest as proto::LspRequestMessage>::Response>,
    {
        let Some((client, project_id)) = self.downstream_client.clone() else {
            return;
        };
        let lsp_data = self.latest_lsp_data(buffer, cx);
        let key = LspKey {
            request_type: TypeId::of::<T>(),
            server_queried: server_id,
        };
        if T::ProtoRequest::stop_previous_requests() {
            if let Some(lsp_requests) = lsp_data.lsp_requests.get_mut(&key) {
                lsp_requests.clear();
            }
        }
        lsp_data.lsp_requests.entry(key).or_default().insert(
            lsp_request_id,
            cx.spawn(async move |lsp_store, cx| {
                let by_server = fetch_task.await;
                lsp_store
                    .update(cx, |lsp_store, cx| {
                        let response = by_server
                            .into_iter()
                            .map(|(server_id, response)| {
                                (
                                    server_id.to_proto(),
                                    T::response_to_proto(
                                        response,
                                        lsp_store,
                                        sender_id,
                                        &buffer_version,
                                        cx,
                                    )
                                    .into(),
                                )
                            })
                            .collect::<HashMap<_, _>>();
                        if let Err(e) = client.send_lsp_response::<T::ProtoRequest>(
                            project_id,
                            lsp_request_id,
                            response,
                        ) {
                            log::error!(
                                "Failed to send {} LSP response: {e:#}",
                                std::any::type_name::<T>()
                            );
                        }
                    })
                    .ok();
            }),
        );
    }

    pub(super) async fn wait_for_buffer_version<T>(
        lsp_store: &Entity<Self>,
        proto_request: &T::ProtoRequest,
        cx: &mut AsyncApp,
    ) -> Result<(Global, Entity<Buffer>)>
    where
        T: LspCommand,
        T::ProtoRequest: proto::LspRequestMessage,
    {
        let buffer_id = BufferId::new(proto_request.buffer_id())?;
        let version = language::proto::deserialize_version(proto_request.buffer_version());
        let buffer = lsp_store.update(cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        buffer
            .update(cx, |buffer, _| buffer.wait_for_version(version.clone()))
            .await?;
        let buffer_version = buffer.read_with(cx, |buffer, _| buffer.version());
        Ok((buffer_version, buffer))
    }
}
