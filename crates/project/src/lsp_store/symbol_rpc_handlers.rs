use super::*;

impl LspStore {
    pub(super) async fn handle_open_buffer_for_symbol(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OpenBufferForSymbol>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenBufferForSymbolResponse> {
        let peer_id = envelope.original_sender_id().unwrap_or_default();
        let symbol = envelope.payload.symbol.context("invalid symbol")?;
        let symbol = Self::deserialize_symbol(symbol)?;
        this.read_with(&cx, |this, _| {
            if let SymbolLocation::OutsideProject {
                abs_path,
                signature,
            } = &symbol.path
            {
                let new_signature = this.symbol_signature(&abs_path);
                anyhow::ensure!(&new_signature == signature, "invalid symbol signature");
            }
            Ok(())
        })?;
        let buffer = this
            .update(&mut cx, |this, cx| {
                this.open_buffer_for_symbol(
                    &Symbol {
                        language_server_name: symbol.language_server_name,
                        source_worktree_id: symbol.source_worktree_id,
                        source_language_server_id: symbol.source_language_server_id,
                        path: symbol.path,
                        name: symbol.name,
                        kind: symbol.kind,
                        range: symbol.range,
                        label: CodeLabel::default(),
                        container_name: symbol.container_name,
                    },
                    cx,
                )
            })
            .await?;

        this.update(&mut cx, |this, cx| {
            let is_private = buffer
                .read(cx)
                .file()
                .map(|f| f.is_private())
                .unwrap_or_default();
            if is_private {
                Err(anyhow!(rpc::ErrorCode::UnsharedItem))
            } else {
                this.buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.create_buffer_for_peer(&buffer, peer_id, cx)
                    })
                    .detach_and_log_err(cx);
                let buffer_id = buffer.read(cx).remote_id().to_proto();
                Ok(proto::OpenBufferForSymbolResponse { buffer_id })
            }
        })
    }

    pub(super) fn symbol_signature(&self, abs_path: &Path) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(abs_path.to_string_lossy().as_bytes());
        hasher.update(self.nonce.to_be_bytes());
        hasher.finalize().as_slice().try_into().unwrap()
    }

    pub(super) async fn handle_get_project_symbols(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetProjectSymbols>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetProjectSymbolsResponse> {
        let symbols = this
            .update(&mut cx, |this, cx| {
                this.symbols(&envelope.payload.query, cx)
            })
            .await?;

        Ok(proto::GetProjectSymbolsResponse {
            symbols: symbols.iter().map(Self::serialize_symbol).collect(),
        })
    }
}
