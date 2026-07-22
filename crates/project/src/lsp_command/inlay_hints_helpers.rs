use super::*;

impl InlayHints {
    pub async fn lsp_to_project_hint(
        lsp_hint: lsp::InlayHint,
        buffer_handle: &Entity<Buffer>,
        server_id: LanguageServerId,
        resolve_state: ResolveState,
        force_no_type_left_padding: bool,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<InlayHint> {
        let kind = lsp_hint.kind.and_then(|kind| match kind {
            lsp::InlayHintKind::TYPE => Some(InlayHintKind::Type),
            lsp::InlayHintKind::PARAMETER => Some(InlayHintKind::Parameter),
            _ => None,
        });

        let position = buffer_handle.read_with(cx, |buffer, _| {
            let position = buffer.clip_point_utf16(point_from_lsp(lsp_hint.position), Bias::Left);
            if kind == Some(InlayHintKind::Parameter) {
                buffer.anchor_before(position)
            } else {
                buffer.anchor_after(position)
            }
        });
        let label = Self::lsp_inlay_label_to_project(lsp_hint.label, server_id)
            .await
            .context("lsp to project inlay hint conversion")?;
        let padding_left = if force_no_type_left_padding && kind == Some(InlayHintKind::Type) {
            false
        } else {
            lsp_hint.padding_left.unwrap_or(false)
        };

        Ok(InlayHint {
            position,
            padding_left,
            padding_right: lsp_hint.padding_right.unwrap_or(false),
            label,
            kind,
            tooltip: lsp_hint.tooltip.map(|tooltip| match tooltip {
                lsp::InlayHintTooltip::String(s) => InlayHintTooltip::String(s),
                lsp::InlayHintTooltip::MarkupContent(markup_content) => {
                    InlayHintTooltip::MarkupContent(MarkupContent {
                        kind: match markup_content.kind {
                            lsp::MarkupKind::PlainText => HoverBlockKind::PlainText,
                            lsp::MarkupKind::Markdown => HoverBlockKind::Markdown,
                        },
                        value: markup_content.value,
                    })
                }
            }),
            resolve_state,
        })
    }

    async fn lsp_inlay_label_to_project(
        lsp_label: lsp::InlayHintLabel,
        server_id: LanguageServerId,
    ) -> anyhow::Result<InlayHintLabel> {
        let label = match lsp_label {
            lsp::InlayHintLabel::String(s) => InlayHintLabel::String(s),
            lsp::InlayHintLabel::LabelParts(lsp_parts) => {
                let mut parts = Vec::with_capacity(lsp_parts.len());
                for lsp_part in lsp_parts {
                    parts.push(InlayHintLabelPart {
                        value: lsp_part.value,
                        tooltip: lsp_part.tooltip.map(|tooltip| match tooltip {
                            lsp::InlayHintLabelPartTooltip::String(s) => {
                                InlayHintLabelPartTooltip::String(s)
                            }
                            lsp::InlayHintLabelPartTooltip::MarkupContent(markup_content) => {
                                InlayHintLabelPartTooltip::MarkupContent(MarkupContent {
                                    kind: match markup_content.kind {
                                        lsp::MarkupKind::PlainText => HoverBlockKind::PlainText,
                                        lsp::MarkupKind::Markdown => HoverBlockKind::Markdown,
                                    },
                                    value: markup_content.value,
                                })
                            }
                        }),
                        location: Some(server_id).zip(lsp_part.location),
                    });
                }
                InlayHintLabel::LabelParts(parts)
            }
        };

        Ok(label)
    }

    pub fn project_to_proto_hint(response_hint: InlayHint) -> proto::InlayHint {
        let (state, lsp_resolve_state) = match response_hint.resolve_state {
            ResolveState::Resolved => (0, None),
            ResolveState::CanResolve(server_id, resolve_data) => (
                1,
                Some(proto::resolve_state::LspResolveState {
                    server_id: server_id.0 as u64,
                    value: resolve_data.map(|json_data| {
                        serde_json::to_string(&json_data)
                            .expect("failed to serialize resolve json data")
                    }),
                }),
            ),
            ResolveState::Resolving => (2, None),
        };
        let resolve_state = Some(proto::ResolveState {
            state,
            lsp_resolve_state,
        });
        proto::InlayHint {
            position: Some(language::proto::serialize_anchor(&response_hint.position)),
            padding_left: response_hint.padding_left,
            padding_right: response_hint.padding_right,
            label: Some(proto::InlayHintLabel {
                label: Some(match response_hint.label {
                    InlayHintLabel::String(s) => proto::inlay_hint_label::Label::Value(s),
                    InlayHintLabel::LabelParts(label_parts) => {
                        proto::inlay_hint_label::Label::LabelParts(proto::InlayHintLabelParts {
                            parts: label_parts.into_iter().map(|label_part| {
                                let location_url = label_part.location.as_ref().map(|(_, location)| location.uri.to_string());
                                let location_range_start = label_part.location.as_ref().map(|(_, location)| point_from_lsp(location.range.start).0).map(|point| proto::PointUtf16 { row: point.row, column: point.column });
                                let location_range_end = label_part.location.as_ref().map(|(_, location)| point_from_lsp(location.range.end).0).map(|point| proto::PointUtf16 { row: point.row, column: point.column });
                                proto::InlayHintLabelPart {
                                value: label_part.value,
                                tooltip: label_part.tooltip.map(|tooltip| {
                                    let proto_tooltip = match tooltip {
                                        InlayHintLabelPartTooltip::String(s) => proto::inlay_hint_label_part_tooltip::Content::Value(s),
                                        InlayHintLabelPartTooltip::MarkupContent(markup_content) => proto::inlay_hint_label_part_tooltip::Content::MarkupContent(proto::MarkupContent {
                                            is_markdown: markup_content.kind == HoverBlockKind::Markdown,
                                            value: markup_content.value,
                                        }),
                                    };
                                    proto::InlayHintLabelPartTooltip {content: Some(proto_tooltip)}
                                }),
                                location_url,
                                location_range_start,
                                location_range_end,
                                language_server_id: label_part.location.as_ref().map(|(server_id, _)| server_id.0 as u64),
                            }}).collect()
                        })
                    }
                }),
            }),
            kind: response_hint.kind.map(|kind| kind.name().to_string()),
            tooltip: response_hint.tooltip.map(|response_tooltip| {
                let proto_tooltip = match response_tooltip {
                    InlayHintTooltip::String(s) => proto::inlay_hint_tooltip::Content::Value(s),
                    InlayHintTooltip::MarkupContent(markup_content) => {
                        proto::inlay_hint_tooltip::Content::MarkupContent(proto::MarkupContent {
                            is_markdown: markup_content.kind == HoverBlockKind::Markdown,
                            value: markup_content.value,
                        })
                    }
                };
                proto::InlayHintTooltip {
                    content: Some(proto_tooltip),
                }
            }),
            resolve_state,
        }
    }

    pub fn proto_to_project_hint(message_hint: proto::InlayHint) -> anyhow::Result<InlayHint> {
        let resolve_state = message_hint.resolve_state.as_ref().unwrap_or_else(|| {
            panic!("incorrect proto inlay hint message: no resolve state in hint {message_hint:?}",)
        });
        let resolve_state_data = resolve_state
            .lsp_resolve_state.as_ref()
            .map(|lsp_resolve_state| {
                let value = lsp_resolve_state.value.as_deref().map(|value| {
                    serde_json::from_str::<Option<lsp::LSPAny>>(value)
                        .with_context(|| format!("incorrect proto inlay hint message: non-json resolve state {lsp_resolve_state:?}"))
                }).transpose()?.flatten();
                anyhow::Ok((LanguageServerId(lsp_resolve_state.server_id as usize), value))
            })
            .transpose()?;
        let resolve_state = match resolve_state.state {
            0 => ResolveState::Resolved,
            1 => {
                let (server_id, lsp_resolve_state) = resolve_state_data.with_context(|| {
                    format!(
                        "No lsp resolve data for the hint that can be resolved: {message_hint:?}"
                    )
                })?;
                ResolveState::CanResolve(server_id, lsp_resolve_state)
            }
            2 => ResolveState::Resolving,
            invalid => {
                anyhow::bail!("Unexpected resolve state {invalid} for hint {message_hint:?}")
            }
        };
        Ok(InlayHint {
            position: message_hint
                .position
                .and_then(language::proto::deserialize_anchor)
                .context("invalid position")?,
            label: match message_hint
                .label
                .and_then(|label| label.label)
                .context("missing label")?
            {
                proto::inlay_hint_label::Label::Value(s) => InlayHintLabel::String(s),
                proto::inlay_hint_label::Label::LabelParts(parts) => {
                    let mut label_parts = Vec::new();
                    for part in parts.parts {
                        label_parts.push(InlayHintLabelPart {
                            value: part.value,
                            tooltip: part.tooltip.map(|tooltip| match tooltip.content {
                                Some(proto::inlay_hint_label_part_tooltip::Content::Value(s)) => {
                                    InlayHintLabelPartTooltip::String(s)
                                }
                                Some(
                                    proto::inlay_hint_label_part_tooltip::Content::MarkupContent(
                                        markup_content,
                                    ),
                                ) => InlayHintLabelPartTooltip::MarkupContent(MarkupContent {
                                    kind: if markup_content.is_markdown {
                                        HoverBlockKind::Markdown
                                    } else {
                                        HoverBlockKind::PlainText
                                    },
                                    value: markup_content.value,
                                }),
                                None => InlayHintLabelPartTooltip::String(String::new()),
                            }),
                            location: {
                                match part
                                    .location_url
                                    .zip(
                                        part.location_range_start.and_then(|start| {
                                            Some(start..part.location_range_end?)
                                        }),
                                    )
                                    .zip(part.language_server_id)
                                {
                                    Some(((uri, range), server_id)) => Some((
                                        LanguageServerId(server_id as usize),
                                        lsp::Location {
                                            uri: lsp::Uri::from_str(&uri).with_context(|| {
                                                format!("invalid uri in hint part {uri:?}")
                                            })?,
                                            range: lsp::Range::new(
                                                point_to_lsp(PointUtf16::new(
                                                    range.start.row,
                                                    range.start.column,
                                                )),
                                                point_to_lsp(PointUtf16::new(
                                                    range.end.row,
                                                    range.end.column,
                                                )),
                                            ),
                                        },
                                    )),
                                    None => None,
                                }
                            },
                        });
                    }

                    InlayHintLabel::LabelParts(label_parts)
                }
            },
            padding_left: message_hint.padding_left,
            padding_right: message_hint.padding_right,
            kind: message_hint
                .kind
                .as_deref()
                .and_then(InlayHintKind::from_name),
            tooltip: message_hint.tooltip.and_then(|tooltip| {
                Some(match tooltip.content? {
                    proto::inlay_hint_tooltip::Content::Value(s) => InlayHintTooltip::String(s),
                    proto::inlay_hint_tooltip::Content::MarkupContent(markup_content) => {
                        InlayHintTooltip::MarkupContent(MarkupContent {
                            kind: if markup_content.is_markdown {
                                HoverBlockKind::Markdown
                            } else {
                                HoverBlockKind::PlainText
                            },
                            value: markup_content.value,
                        })
                    }
                })
            }),
            resolve_state,
        })
    }

    pub fn project_to_lsp_hint(hint: InlayHint, snapshot: &BufferSnapshot) -> lsp::InlayHint {
        lsp::InlayHint {
            position: point_to_lsp(hint.position.to_point_utf16(snapshot)),
            kind: hint.kind.map(|kind| match kind {
                InlayHintKind::Type => lsp::InlayHintKind::TYPE,
                InlayHintKind::Parameter => lsp::InlayHintKind::PARAMETER,
            }),
            text_edits: None,
            tooltip: hint.tooltip.and_then(|tooltip| {
                Some(match tooltip {
                    InlayHintTooltip::String(s) => lsp::InlayHintTooltip::String(s),
                    InlayHintTooltip::MarkupContent(markup_content) => {
                        lsp::InlayHintTooltip::MarkupContent(lsp::MarkupContent {
                            kind: match markup_content.kind {
                                HoverBlockKind::PlainText => lsp::MarkupKind::PlainText,
                                HoverBlockKind::Markdown => lsp::MarkupKind::Markdown,
                                HoverBlockKind::Code { .. } => return None,
                            },
                            value: markup_content.value,
                        })
                    }
                })
            }),
            label: match hint.label {
                InlayHintLabel::String(s) => lsp::InlayHintLabel::String(s),
                InlayHintLabel::LabelParts(label_parts) => lsp::InlayHintLabel::LabelParts(
                    label_parts
                        .into_iter()
                        .map(|part| lsp::InlayHintLabelPart {
                            value: part.value,
                            tooltip: part.tooltip.and_then(|tooltip| {
                                Some(match tooltip {
                                    InlayHintLabelPartTooltip::String(s) => {
                                        lsp::InlayHintLabelPartTooltip::String(s)
                                    }
                                    InlayHintLabelPartTooltip::MarkupContent(markup_content) => {
                                        lsp::InlayHintLabelPartTooltip::MarkupContent(
                                            lsp::MarkupContent {
                                                kind: match markup_content.kind {
                                                    HoverBlockKind::PlainText => {
                                                        lsp::MarkupKind::PlainText
                                                    }
                                                    HoverBlockKind::Markdown => {
                                                        lsp::MarkupKind::Markdown
                                                    }
                                                    HoverBlockKind::Code { .. } => return None,
                                                },
                                                value: markup_content.value,
                                            },
                                        )
                                    }
                                })
                            }),
                            location: part.location.map(|(_, location)| location),
                            command: None,
                        })
                        .collect(),
                ),
            },
            padding_left: Some(hint.padding_left),
            padding_right: Some(hint.padding_right),
            data: match hint.resolve_state {
                ResolveState::CanResolve(_, data) => data,
                ResolveState::Resolving | ResolveState::Resolved => None,
            },
        }
    }

    pub fn can_resolve_inlays(capabilities: &ServerCapabilities) -> bool {
        capabilities
            .inlay_hint_provider
            .as_ref()
            .and_then(|options| match options {
                OneOf::Left(_is_supported) => None,
                OneOf::Right(capabilities) => match capabilities {
                    lsp::InlayHintServerCapabilities::Options(o) => o.resolve_provider,
                    lsp::InlayHintServerCapabilities::RegistrationOptions(o) => {
                        o.inlay_hint_options.resolve_provider
                    }
                },
            })
            .unwrap_or(false)
    }

    pub fn check_capabilities(capabilities: &ServerCapabilities) -> bool {
        capabilities
            .inlay_hint_provider
            .as_ref()
            .is_some_and(|inlay_hint_provider| match inlay_hint_provider {
                lsp::OneOf::Left(enabled) => *enabled,
                lsp::OneOf::Right(_) => true,
            })
    }
}
