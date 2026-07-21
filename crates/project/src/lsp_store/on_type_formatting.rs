use super::*;

impl LspStore {
    fn apply_on_type_formatting(
        &mut self,
        buffer: Entity<Buffer>,
        position: Anchor,
        trigger: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            if !self.check_if_capable_for_proto_request(
                &buffer,
                |capabilities| {
                    OnTypeFormatting::supports_on_type_formatting(&trigger, capabilities)
                },
                cx,
            ) {
                return Task::ready(Ok(None));
            }
            let request = proto::OnTypeFormatting {
                project_id,
                buffer_id: buffer.read(cx).remote_id().into(),
                position: Some(serialize_anchor(&position)),
                trigger,
                version: serialize_version(&buffer.read(cx).version()),
            };
            cx.background_spawn(async move {
                client
                    .request(request)
                    .await?
                    .transaction
                    .map(language::proto::deserialize_transaction)
                    .transpose()
            })
        } else if let Some(local) = self.as_local_mut() {
            let buffer_id = buffer.read(cx).remote_id();
            local.buffers_being_formatted.insert(buffer_id);
            cx.spawn(async move |this, cx| {
                let _cleanup = defer({
                    let this = this.clone();
                    let mut cx = cx.clone();
                    move || {
                        this.update(&mut cx, |this, _| {
                            if let Some(local) = this.as_local_mut() {
                                local.buffers_being_formatted.remove(&buffer_id);
                            }
                        })
                        .ok();
                    }
                });

                buffer
                    .update(cx, |buffer, _| {
                        buffer.wait_for_edits(Some(position.timestamp()))
                    })
                    .await?;
                this.update(cx, |this, cx| {
                    let position = position.to_point_utf16(buffer.read(cx));
                    this.on_type_format(buffer, position, trigger, false, cx)
                })?
                .await
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }

    pub fn on_type_format<T: ToPointUtf16>(
        &mut self,
        buffer: Entity<Buffer>,
        position: T,
        trigger: String,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.on_type_format_impl(buffer, position, trigger, push_to_history, cx)
    }

    fn on_type_format_impl(
        &mut self,
        buffer: Entity<Buffer>,
        position: PointUtf16,
        trigger: String,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        let options = buffer.update(cx, |buffer, cx| {
            lsp_command::lsp_formatting_options(
                LanguageSettings::for_buffer_at(buffer, position, cx).as_ref(),
            )
        });

        cx.spawn(async move |this, cx| {
            if let Some(waiter) =
                buffer.update(cx, |buffer, _| buffer.wait_for_autoindent_applied())
            {
                waiter.await?;
            }
            cx.update(|cx| {
                this.update(cx, |this, cx| {
                    this.request_lsp(
                        buffer.clone(),
                        LanguageServerToQuery::FirstCapable,
                        OnTypeFormatting {
                            position,
                            trigger,
                            options,
                            push_to_history,
                        },
                        cx,
                    )
                })
            })?
            .await
        })
    }

    pub(super) async fn handle_on_type_formatting(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OnTypeFormatting>,
        mut cx: AsyncApp,
    ) -> Result<proto::OnTypeFormattingResponse> {
        let on_type_formatting = this.update(&mut cx, |this, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
            let position = envelope
                .payload
                .position
                .and_then(deserialize_anchor)
                .context("invalid position")?;
            anyhow::Ok(this.apply_on_type_formatting(
                buffer,
                position,
                envelope.payload.trigger.clone(),
                cx,
            ))
        })?;

        let transaction = on_type_formatting
            .await?
            .as_ref()
            .map(language::proto::serialize_transaction);
        Ok(proto::OnTypeFormattingResponse { transaction })
    }
}
