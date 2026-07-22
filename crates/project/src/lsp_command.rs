mod code_actions;
mod code_lens;
mod completions;
mod definition_commands;
mod diagnostics;
mod document_color;
mod document_diagnostics_command;
mod document_highlights;
mod document_links;
mod document_symbols;
mod folding_ranges;
mod formatting;
mod hover;
mod inlay_hints_command;
mod inlay_hints_helpers;
mod linked_editing;
mod location_links;
mod references;
mod rename_commands;
mod semantic_tokens;
mod signature_command;
pub mod signature_help;
mod types;

use crate::{
    CodeAction, CompletionSource, CoreCompletion, CoreCompletionResponse, DocumentColor,
    DocumentHighlight, DocumentSymbol, Hover, HoverBlock, HoverBlockKind, InlayHint,
    InlayHintLabel, InlayHintLabelPart, InlayHintLabelPartTooltip, InlayHintTooltip, Location,
    LocationLink, LspAction, LspPullDiagnostics, MarkupContent, PrepareRenameResponse,
    ProjectTransaction, PulledDiagnostics, ResolveState,
    lsp_store::{LocalLspStore, LspDocumentLink, LspFoldingRange, LspStore},
};
use anyhow::{Context as _, Result};
use client::proto::{self, PeerId};
use clock::Global;
use collections::HashMap;
use diagnostics::*;
use futures::future;
use gpui::{App, AsyncApp, Entity, SharedString, Task, TaskExt, prelude::FluentBuilder};
use language::{
    Anchor, Bias, Buffer, BufferSnapshot, CachedLspAdapter, CharKind, CharScopeContext,
    OffsetRangeExt, PointUtf16, ToOffset, ToPointUtf16, Transaction, Unclipped,
    language_settings::{InlayHintKind, LanguageSettings},
    point_from_lsp, point_to_lsp,
    proto::{
        deserialize_anchor, deserialize_anchor_range, deserialize_version, serialize_anchor,
        serialize_anchor_range, serialize_version,
    },
    range_from_lsp, range_to_lsp,
};
use lsp::{
    AdapterServerCapabilities, CodeDescription, CompletionContext,
    CompletionListItemDefaultsEditRange, CompletionTriggerKind, DocumentHighlightKind,
    LanguageServer, LanguageServerId, LinkedEditingRangeServerCapabilities, OneOf, RenameOptions,
    ServerCapabilities,
};
use serde_json::Value;

use signature_help::{lsp_to_proto_signature, proto_to_lsp_signature};
use std::{
    cmp::Reverse, collections::hash_map, mem, ops::Range, path::Path, str::FromStr, sync::Arc,
};
use text::{BufferId, LineEnding};
pub(crate) use types::*;
use util::{ResultExt as _, debug_panic};

pub(crate) use completions::parse_completion_text_edit;
use location_links::language_server_for_buffer;
pub use location_links::{
    location_link_from_lsp, location_link_from_proto, location_link_to_proto,
    location_links_from_lsp, location_links_from_proto, location_links_to_proto,
};
pub use signature_help::SignatureHelp;

pub fn lsp_formatting_options(settings: &LanguageSettings) -> lsp::FormattingOptions {
    lsp::FormattingOptions {
        tab_size: settings.tab_size.into(),
        insert_spaces: !settings.hard_tabs,
        trim_trailing_whitespace: Some(settings.remove_trailing_whitespace_on_save),
        trim_final_newlines: Some(settings.ensure_final_newline_on_save),
        insert_final_newline: Some(settings.ensure_final_newline_on_save),
        ..lsp::FormattingOptions::default()
    }
}

pub fn file_path_to_lsp_url(path: &Path) -> Result<lsp::Uri> {
    match lsp::Uri::from_file_path(path) {
        Ok(url) => Ok(url),
        Err(()) => anyhow::bail!("Invalid file path provided to LSP request: {path:?}"),
    }
}

pub(crate) fn make_text_document_identifier(path: &Path) -> Result<lsp::TextDocumentIdentifier> {
    Ok(lsp::TextDocumentIdentifier {
        uri: file_path_to_lsp_url(path)?,
    })
}

pub(crate) fn make_lsp_text_document_position(
    path: &Path,
    position: PointUtf16,
) -> Result<lsp::TextDocumentPositionParams> {
    Ok(lsp::TextDocumentPositionParams {
        text_document: make_text_document_identifier(path)?,
        position: point_to_lsp(position),
    })
}

pub trait LspCommand: 'static + Sized + Send + std::fmt::Debug {
    type Response: 'static + Default + Send + std::fmt::Debug;
    type LspRequest: 'static + Send + lsp::request::Request;
    type ProtoRequest: 'static + Send + proto::RequestMessage;

    fn display_name(&self) -> &str;

    fn status(&self) -> Option<String> {
        None
    }

    fn to_lsp_params_or_response(
        &self,
        path: &Path,
        buffer: &Buffer,
        language_server: &Arc<LanguageServer>,
        cx: &App,
    ) -> Result<
        LspParamsOrResponse<<Self::LspRequest as lsp::request::Request>::Params, Self::Response>,
    > {
        if self.check_capabilities(language_server.adapter_server_capabilities()) {
            Ok(LspParamsOrResponse::Params(self.to_lsp(
                path,
                buffer,
                language_server,
                cx,
            )?))
        } else {
            Ok(LspParamsOrResponse::Response(Default::default()))
        }
    }

    /// When false, `to_lsp_params_or_response` default implementation will return the default response.
    fn check_capabilities(&self, _: AdapterServerCapabilities) -> bool;

    fn to_lsp(
        &self,
        path: &Path,
        buffer: &Buffer,
        language_server: &Arc<LanguageServer>,
        cx: &App,
    ) -> Result<<Self::LspRequest as lsp::request::Request>::Params>;

    async fn response_from_lsp(
        self,
        message: <Self::LspRequest as lsp::request::Request>::Result,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Self::Response>;

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> Self::ProtoRequest;

    async fn from_proto(
        message: Self::ProtoRequest,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Self>;

    fn response_to_proto(
        response: Self::Response,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        buffer_version: &clock::Global,
        cx: &mut App,
    ) -> <Self::ProtoRequest as proto::RequestMessage>::Response;

    async fn response_from_proto(
        self,
        message: <Self::ProtoRequest as proto::RequestMessage>::Response,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Self::Response>;

    fn buffer_id_from_proto(message: &Self::ProtoRequest) -> Result<BufferId>;
}

pub enum LspParamsOrResponse<P, R> {
    Params(P),
    Response(R),
}
