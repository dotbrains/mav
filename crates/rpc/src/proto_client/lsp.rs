use anyhow::{Context, Result};
use collections::HashMap;
use futures::channel::oneshot;
use gpui::{BackgroundExecutor, FutureExt as _};
use proto::{AnyTypedEnvelope, EnvelopedMessage, LspRequestId, LspRequestMessage, TypedEnvelope};
use std::{sync::atomic, time::Duration};

use super::AnyProtoClient;

impl AnyProtoClient {
    pub fn request_lsp<T>(
        &self,
        project_id: u64,
        server_id: Option<u64>,
        timeout: Duration,
        executor: BackgroundExecutor,
        request: T,
    ) -> impl Future<
        Output = Result<Option<TypedEnvelope<Vec<proto::ProtoLspResponse<T::Response>>>>>,
    > + use<T>
    where
        T: LspRequestMessage,
    {
        let new_id = LspRequestId(
            self.0
                .next_lsp_request_id
                .fetch_add(1, atomic::Ordering::Acquire),
        );
        let (tx, rx) = oneshot::channel();
        {
            self.0.request_ids.lock().insert(new_id, tx);
        }

        let query = proto::LspQuery {
            project_id,
            server_id,
            lsp_request_id: new_id.0,
            request: Some(request.to_proto_query()),
        };
        let request = self.request(query);
        let request_ids = self.0.request_ids.clone();
        async move {
            match request.await {
                Ok(_request_enqueued) => {}
                Err(e) => {
                    request_ids.lock().remove(&new_id);
                    return Err(e).context("sending LSP proto request");
                }
            }

            let response = rx.with_timeout(timeout, &executor).await;
            {
                request_ids.lock().remove(&new_id);
            }
            match response {
                Ok(Ok(response)) => {
                    let response = response
                        .context("waiting for LSP proto response")?
                        .map(|response| {
                            anyhow::Ok(TypedEnvelope {
                                payload: response
                                    .payload
                                    .into_iter()
                                    .map(|lsp_response| lsp_response.into_response::<T>())
                                    .collect::<Result<Vec<_>>>()?,
                                sender_id: response.sender_id,
                                original_sender_id: response.original_sender_id,
                                message_id: response.message_id,
                                received_at: response.received_at,
                            })
                        })
                        .transpose()
                        .context("converting LSP proto response")?;
                    Ok(response)
                }
                Err(_cancelled_due_timeout) => Ok(None),
                Ok(Err(_channel_dropped)) => Ok(None),
            }
        }
    }

    pub fn send_lsp_response<T: LspRequestMessage>(
        &self,
        project_id: u64,
        lsp_request_id: LspRequestId,
        server_responses: HashMap<u64, T::Response>,
    ) -> Result<()> {
        self.send(proto::LspQueryResponse {
            project_id,
            lsp_request_id: lsp_request_id.0,
            responses: server_responses
                .into_iter()
                .map(|(server_id, response)| proto::LspResponse {
                    server_id,
                    response: Some(T::response_to_proto_query(response)),
                })
                .collect(),
        })
    }

    pub fn handle_lsp_response(&self, mut envelope: TypedEnvelope<proto::LspQueryResponse>) {
        let request_id = LspRequestId(envelope.payload.lsp_request_id);
        let mut response_senders = self.0.request_ids.lock();
        if let Some(tx) = response_senders.remove(&request_id) {
            let responses = envelope.payload.responses.drain(..).collect::<Vec<_>>();
            tx.send(Ok(Some(proto::TypedEnvelope {
                sender_id: envelope.sender_id,
                original_sender_id: envelope.original_sender_id,
                message_id: envelope.message_id,
                received_at: envelope.received_at,
                payload: responses
                    .into_iter()
                    .filter_map(|response| {
                        use proto::lsp_response::Response;

                        let server_id = response.server_id;
                        let response = match response.response? {
                            Response::GetReferencesResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDocumentColorResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetHoverResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetCodeActionsResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetSignatureHelpResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetCodeLensResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDocumentDiagnosticsResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDefinitionResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDeclarationResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetTypeDefinitionResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetImplementationResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::InlayHintsResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::SemanticTokensResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetFoldingRangesResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDocumentSymbolsResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                            Response::GetDocumentLinksResponse(response) => {
                                to_any_envelope(&envelope, response)
                            }
                        };
                        Some(proto::ProtoLspResponse {
                            server_id,
                            response,
                        })
                    })
                    .collect(),
            })))
            .ok();
        }
    }
}

fn to_any_envelope<T: EnvelopedMessage>(
    envelope: &TypedEnvelope<proto::LspQueryResponse>,
    response: T,
) -> Box<dyn AnyTypedEnvelope> {
    Box::new(proto::TypedEnvelope {
        sender_id: envelope.sender_id,
        original_sender_id: envelope.original_sender_id,
        message_id: envelope.message_id,
        received_at: envelope.received_at,
        payload: response,
    }) as Box<_>
}
