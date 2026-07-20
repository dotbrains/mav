use crate::{lsp_command::LspCommand, lsp_store::LspStore, make_text_document_identifier};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use gpui::{App, AsyncApp, Entity};
use language::{Buffer, point_to_lsp, proto::deserialize_anchor};
use lsp::{AdapterServerCapabilities, LanguageServer, LanguageServerId};
use rpc::proto::{self, PeerId};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use text::{BufferId, PointUtf16, ToPointUtf16};

pub mod navigation;
pub mod runnables;
pub use navigation::{
    GoToParentModule, LspGoToParentModule, LspSwitchSourceHeader, SwitchSourceHeader,
    SwitchSourceHeaderParams, SwitchSourceHeaderResult,
};
pub use runnables::{
    CargoRunnableArgs, GetLspRunnables, LspRunnables, Runnable, RunnableArgs, Runnables,
    ShellRunnableArgs, runnable_to_task_template,
};

pub enum LspExtExpandMacro {}

impl lsp::request::Request for LspExtExpandMacro {
    type Params = ExpandMacroParams;
    type Result = Option<ExpandedMacro>;
    const METHOD: &'static str = "rust-analyzer/expandMacro";
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExpandMacroParams {
    pub text_document: lsp::TextDocumentIdentifier,
    pub position: lsp::Position,
}

#[derive(Default, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExpandedMacro {
    pub name: String,
    pub expansion: String,
}

impl ExpandedMacro {
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.expansion.is_empty()
    }
}
#[derive(Debug)]
pub struct ExpandMacro {
    pub position: PointUtf16,
}

#[async_trait(?Send)]
impl LspCommand for ExpandMacro {
    type Response = ExpandedMacro;
    type LspRequest = LspExtExpandMacro;
    type ProtoRequest = proto::LspExtExpandMacro;

    fn display_name(&self) -> &str {
        "Expand macro"
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
    ) -> Result<ExpandMacroParams> {
        Ok(ExpandMacroParams {
            text_document: make_text_document_identifier(path)?,
            position: point_to_lsp(self.position),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<ExpandedMacro>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> anyhow::Result<ExpandedMacro> {
        Ok(message
            .map(|message| ExpandedMacro {
                name: message.name,
                expansion: message.expansion,
            })
            .unwrap_or_default())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LspExtExpandMacro {
        proto::LspExtExpandMacro {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
        }
    }

    async fn from_proto(
        message: Self::ProtoRequest,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        cx: AsyncApp,
    ) -> anyhow::Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        response: ExpandedMacro,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::LspExtExpandMacroResponse {
        proto::LspExtExpandMacroResponse {
            name: response.name,
            expansion: response.expansion,
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LspExtExpandMacroResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> anyhow::Result<ExpandedMacro> {
        Ok(ExpandedMacro {
            name: message.name,
            expansion: message.expansion,
        })
    }

    fn buffer_id_from_proto(message: &proto::LspExtExpandMacro) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

pub enum LspOpenDocs {}

impl lsp::request::Request for LspOpenDocs {
    type Params = OpenDocsParams;
    type Result = Option<DocsUrls>;
    const METHOD: &'static str = "experimental/externalDocs";
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocsParams {
    pub text_document: lsp::TextDocumentIdentifier,
    pub position: lsp::Position,
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocsUrls {
    pub web: Option<String>,
    pub local: Option<String>,
}

impl DocsUrls {
    pub fn is_empty(&self) -> bool {
        self.web.is_none() && self.local.is_none()
    }
}

#[derive(Debug)]
pub struct OpenDocs {
    pub position: PointUtf16,
}

#[async_trait(?Send)]
impl LspCommand for OpenDocs {
    type Response = DocsUrls;
    type LspRequest = LspOpenDocs;
    type ProtoRequest = proto::LspExtOpenDocs;

    fn display_name(&self) -> &str {
        "Open docs"
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
    ) -> Result<OpenDocsParams> {
        let uri = lsp::Uri::from_file_path(path)
            .map_err(|()| anyhow::anyhow!("{path:?} is not a valid URI"))?;
        Ok(OpenDocsParams {
            text_document: lsp::TextDocumentIdentifier { uri },
            position: point_to_lsp(self.position),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<DocsUrls>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> anyhow::Result<DocsUrls> {
        Ok(message
            .map(|message| DocsUrls {
                web: message.web,
                local: message.local,
            })
            .unwrap_or_default())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LspExtOpenDocs {
        proto::LspExtOpenDocs {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
        }
    }

    async fn from_proto(
        message: Self::ProtoRequest,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        cx: AsyncApp,
    ) -> anyhow::Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        response: DocsUrls,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::LspExtOpenDocsResponse {
        proto::LspExtOpenDocsResponse {
            web: response.web,
            local: response.local,
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LspExtOpenDocsResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> anyhow::Result<DocsUrls> {
        Ok(DocsUrls {
            web: message.web,
            local: message.local,
        })
    }

    fn buffer_id_from_proto(message: &proto::LspExtOpenDocs) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

#[derive(Debug)]
pub struct LspExtCancelFlycheck {}

#[derive(Debug)]
pub struct LspExtRunFlycheck {}

#[derive(Debug)]
pub struct LspExtClearFlycheck {}

impl lsp::notification::Notification for LspExtCancelFlycheck {
    type Params = ();
    const METHOD: &'static str = "rust-analyzer/cancelFlycheck";
}

impl lsp::notification::Notification for LspExtRunFlycheck {
    type Params = RunFlycheckParams;
    const METHOD: &'static str = "rust-analyzer/runFlycheck";
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RunFlycheckParams {
    pub text_document: Option<lsp::TextDocumentIdentifier>,
}

impl lsp::notification::Notification for LspExtClearFlycheck {
    type Params = ();
    const METHOD: &'static str = "rust-analyzer/clearFlycheck";
}
