use super::*;

impl LspStore {
    pub(super) async fn handle_lsp_get_completions(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetCompletions>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetCompletionsResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();

        let buffer_id = GetCompletions::buffer_id_from_proto(&envelope.payload)?;
        let buffer_handle = this.update(&mut cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let request = GetCompletions::from_proto(
            envelope.payload,
            this.clone(),
            buffer_handle.clone(),
            cx.clone(),
        )
        .await?;

        let server_to_query = match request.server_id {
            Some(server_id) => LanguageServerToQuery::Other(server_id),
            None => LanguageServerToQuery::FirstCapable,
        };

        let response = this
            .update(&mut cx, |this, cx| {
                this.request_lsp(buffer_handle.clone(), server_to_query, request, cx)
            })
            .await?;
        this.update(&mut cx, |this, cx| {
            Ok(GetCompletions::response_to_proto(
                response,
                this,
                sender_id,
                &buffer_handle.read(cx).version(),
                cx,
            ))
        })
    }

    pub(super) async fn handle_lsp_command<T: LspCommand>(
        this: Entity<Self>,
        envelope: TypedEnvelope<T::ProtoRequest>,
        mut cx: AsyncApp,
    ) -> Result<<T::ProtoRequest as proto::RequestMessage>::Response>
    where
        <T::LspRequest as lsp::request::Request>::Params: Send,
        <T::LspRequest as lsp::request::Request>::Result: Send,
    {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let buffer_id = T::buffer_id_from_proto(&envelope.payload)?;
        let buffer_handle = this.update(&mut cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let request = T::from_proto(
            envelope.payload,
            this.clone(),
            buffer_handle.clone(),
            cx.clone(),
        )
        .await?;
        let response = this
            .update(&mut cx, |this, cx| {
                this.request_lsp(
                    buffer_handle.clone(),
                    LanguageServerToQuery::FirstCapable,
                    request,
                    cx,
                )
            })
            .await?;
        this.update(&mut cx, |this, cx| {
            Ok(T::response_to_proto(
                response,
                this,
                sender_id,
                &buffer_handle.read(cx).version(),
                cx,
            ))
        })
    }
}
