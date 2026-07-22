use crate::{
    LocationLink,
    lsp_command::{
        LspCommand, location_links_from_lsp, location_links_from_proto, location_links_to_proto,
    },
    lsp_store::LspStore,
    make_lsp_text_document_position, make_text_document_identifier,
};
use anyhow::{Context as _, Result};
use gpui::{App, AsyncApp, Entity};
use language::{Buffer, proto::deserialize_anchor};
use lsp::{AdapterServerCapabilities, LanguageServer, LanguageServerId};
use rpc::proto::{self, PeerId};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use text::{BufferId, PointUtf16, ToPointUtf16};

pub enum LspSwitchSourceHeader {}

impl lsp::request::Request for LspSwitchSourceHeader {
    type Params = SwitchSourceHeaderParams;
    type Result = Option<SwitchSourceHeaderResult>;
    const METHOD: &'static str = "textDocument/switchSourceHeader";
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwitchSourceHeaderParams(lsp::TextDocumentIdentifier);

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SwitchSourceHeaderResult(pub String);

#[derive(Default, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwitchSourceHeader;

#[derive(Debug)]
pub struct GoToParentModule {
    pub position: PointUtf16,
}

pub struct LspGoToParentModule {}

impl lsp::request::Request for LspGoToParentModule {
    type Params = lsp::TextDocumentPositionParams;
    type Result = Option<Vec<lsp::LocationLink>>;
    const METHOD: &'static str = "experimental/parentModule";
}

impl LspCommand for SwitchSourceHeader {
    type Response = SwitchSourceHeaderResult;
    type LspRequest = LspSwitchSourceHeader;
    type ProtoRequest = proto::LspExtSwitchSourceHeader;

    fn display_name(&self) -> &str {
        "Switch source header"
    }

    fn check_capabilities(&self, _: AdapterServerCapabilities) -> bool {
        true
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<SwitchSourceHeaderParams> {
        Ok(SwitchSourceHeaderParams(make_text_document_identifier(
            path,
        )?))
    }

    async fn response_from_lsp(
        self,
        message: Option<SwitchSourceHeaderResult>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> anyhow::Result<SwitchSourceHeaderResult> {
        Ok(message
            .map(|message| SwitchSourceHeaderResult(message.0))
            .unwrap_or_default())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LspExtSwitchSourceHeader {
        proto::LspExtSwitchSourceHeader {
            project_id,
            buffer_id: buffer.remote_id().into(),
        }
    }

    async fn from_proto(
        _: Self::ProtoRequest,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> anyhow::Result<Self> {
        Ok(Self {})
    }

    fn response_to_proto(
        response: SwitchSourceHeaderResult,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::LspExtSwitchSourceHeaderResponse {
        proto::LspExtSwitchSourceHeaderResponse {
            target_file: response.0,
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LspExtSwitchSourceHeaderResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> anyhow::Result<SwitchSourceHeaderResult> {
        Ok(SwitchSourceHeaderResult(message.target_file))
    }

    fn buffer_id_from_proto(message: &proto::LspExtSwitchSourceHeader) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl LspCommand for GoToParentModule {
    type Response = Vec<LocationLink>;
    type LspRequest = LspGoToParentModule;
    type ProtoRequest = proto::LspExtGoToParentModule;

    fn display_name(&self) -> &str {
        "Go to parent module"
    }

    fn check_capabilities(&self, _: AdapterServerCapabilities) -> bool {
        true
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::TextDocumentPositionParams> {
        make_lsp_text_document_position(path, self.position)
    }

    async fn response_from_lsp(
        self,
        links: Option<Vec<lsp::LocationLink>>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> anyhow::Result<Vec<LocationLink>> {
        location_links_from_lsp(
            links.map(lsp::GotoDefinitionResponse::Link),
            lsp_store,
            buffer,
            server_id,
            false,
            cx,
        )
        .await
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LspExtGoToParentModule {
        proto::LspExtGoToParentModule {
            project_id,
            buffer_id: buffer.remote_id().to_proto(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
        }
    }

    async fn from_proto(
        request: Self::ProtoRequest,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        cx: AsyncApp,
    ) -> anyhow::Result<Self> {
        let position = request
            .position
            .and_then(deserialize_anchor)
            .context("bad request with bad position")?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        links: Vec<LocationLink>,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::LspExtGoToParentModuleResponse {
        proto::LspExtGoToParentModuleResponse {
            links: location_links_to_proto(links, lsp_store, peer_id, cx),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LspExtGoToParentModuleResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> anyhow::Result<Vec<LocationLink>> {
        location_links_from_proto(message.links, lsp_store, cx).await
    }

    fn buffer_id_from_proto(message: &proto::LspExtGoToParentModule) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
