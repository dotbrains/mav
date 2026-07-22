use crate::{
    LocationLink,
    lsp_command::{
        LspCommand, file_path_to_lsp_url, location_link_from_lsp, location_link_from_proto,
        location_link_to_proto,
    },
    lsp_store::LspStore,
};
use anyhow::{Context as _, Result};
use collections::HashMap;
use gpui::{App, AsyncApp, Entity};
use language::{
    Buffer, point_to_lsp,
    proto::{deserialize_anchor, serialize_anchor},
};
use lsp::{AdapterServerCapabilities, LanguageServer, LanguageServerId};
use rpc::proto::{self, PeerId};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use task::TaskTemplate;
use text::{BufferId, ToPointUtf16};

// https://rust-analyzer.github.io/book/contributing/lsp-extensions.html#runnables
// Taken from https://github.com/rust-lang/rust-analyzer/blob/3aaa35b49ef27e15144952aa4f7ba3eecd36fbb4/crates/rust-analyzer/src/lsp/ext.rs#L425-L489
//
// rust-analyzer keeps RunnableKind synced with RunnableArgs, so we rely on the
// JSON kind field to avoid serde(untagged) confusion between shell and cargo args.
pub enum Runnables {}

impl lsp::request::Request for Runnables {
    type Params = RunnablesParams;
    type Result = Vec<Runnable>;
    const METHOD: &'static str = "experimental/runnables";
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunnablesParams {
    pub text_document: lsp::TextDocumentIdentifier,
    #[serde(default)]
    pub position: Option<lsp::Position>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Runnable {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<lsp::LocationLink>,
    #[serde(flatten)]
    pub args: RunnableArgs,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "kind", content = "args")]
#[serde(rename_all = "lowercase")]
pub enum RunnableArgs {
    Cargo(CargoRunnableArgs),
    Shell(ShellRunnableArgs),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CargoRunnableArgs {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub override_cargo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub cargo_args: Vec<String>,
    #[serde(default)]
    pub executable_args: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ShellRunnableArgs {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    pub cwd: PathBuf,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug)]
pub struct GetLspRunnables {
    pub buffer_id: BufferId,
    pub position: Option<text::Anchor>,
}

#[derive(Debug, Default)]
pub struct LspRunnables {
    pub runnables: Vec<(Option<LocationLink>, TaskTemplate)>,
}

pub fn runnable_to_task_template(label: String, args: RunnableArgs) -> TaskTemplate {
    let mut task_template = TaskTemplate {
        label,
        ..Default::default()
    };
    match args {
        RunnableArgs::Cargo(cargo) => {
            match cargo.override_cargo {
                Some(override_cargo) => {
                    let mut override_parts = override_cargo.split(" ").map(|s| s.to_string());
                    task_template.command = override_parts
                        .next()
                        .unwrap_or_else(|| override_cargo.clone());
                    task_template.args.extend(override_parts);
                }
                None => task_template.command = "cargo".to_string(),
            };
            task_template.env = cargo.environment;
            task_template.cwd = Some(
                cargo
                    .workspace_root
                    .unwrap_or(cargo.cwd)
                    .to_string_lossy()
                    .to_string(),
            );
            task_template.args.extend(cargo.cargo_args);
            if !cargo.executable_args.is_empty() {
                let shell_kind = task_template.shell.shell_kind(cfg!(windows));
                task_template.args.push("--".to_string());
                task_template.args.extend(
                    cargo.executable_args.into_iter().flat_map(|extra_arg| {
                        shell_kind.try_quote(&extra_arg).map(|s| s.to_string())
                    }),
                );
            }
        }
        RunnableArgs::Shell(shell) => {
            task_template.command = shell.program;
            task_template.args = shell.args;
            task_template.env = shell.environment;
            task_template.cwd = Some(shell.cwd.to_string_lossy().into_owned());
        }
    }
    task_template
}

impl LspCommand for GetLspRunnables {
    type Response = LspRunnables;
    type LspRequest = Runnables;
    type ProtoRequest = proto::LspExtRunnables;

    fn display_name(&self) -> &str {
        "LSP Runnables"
    }

    fn check_capabilities(&self, _: AdapterServerCapabilities) -> bool {
        true
    }

    fn to_lsp(
        &self,
        path: &Path,
        buffer: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<RunnablesParams> {
        let url = file_path_to_lsp_url(path)?;
        Ok(RunnablesParams {
            text_document: lsp::TextDocumentIdentifier::new(url),
            position: self
                .position
                .map(|anchor| point_to_lsp(anchor.to_point_utf16(&buffer.snapshot()))),
        })
    }

    async fn response_from_lsp(
        self,
        lsp_runnables: Vec<Runnable>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        mut cx: AsyncApp,
    ) -> Result<LspRunnables> {
        let mut runnables = Vec::with_capacity(lsp_runnables.len());

        for runnable in lsp_runnables {
            let location = match runnable.location {
                Some(location) => Some(
                    location_link_from_lsp(location, &lsp_store, &buffer, server_id, &mut cx)
                        .await?,
                ),
                None => None,
            };
            let task_template = runnable_to_task_template(runnable.label, runnable.args);
            runnables.push((location, task_template));
        }

        Ok(LspRunnables { runnables })
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LspExtRunnables {
        proto::LspExtRunnables {
            project_id,
            buffer_id: buffer.remote_id().to_proto(),
            position: self.position.as_ref().map(serialize_anchor),
        }
    }

    async fn from_proto(
        message: proto::LspExtRunnables,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self> {
        let buffer_id = Self::buffer_id_from_proto(&message)?;
        let position = message.position.and_then(deserialize_anchor);
        Ok(Self {
            buffer_id,
            position,
        })
    }

    fn response_to_proto(
        response: LspRunnables,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::LspExtRunnablesResponse {
        proto::LspExtRunnablesResponse {
            runnables: response
                .runnables
                .into_iter()
                .map(|(location, task_template)| proto::LspRunnable {
                    location: location
                        .map(|location| location_link_to_proto(location, lsp_store, peer_id, cx)),
                    task_template: serde_json::to_vec(&task_template).unwrap(),
                })
                .collect(),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LspExtRunnablesResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<LspRunnables> {
        let mut runnables = LspRunnables {
            runnables: Vec::new(),
        };

        for lsp_runnable in message.runnables {
            let location = match lsp_runnable.location {
                Some(location) => {
                    Some(location_link_from_proto(location, lsp_store.clone(), &mut cx).await?)
                }
                None => None,
            };
            let task_template = serde_json::from_slice(&lsp_runnable.task_template)
                .context("deserializing task template from proto")?;
            runnables.runnables.push((location, task_template));
        }

        Ok(runnables)
    }

    fn buffer_id_from_proto(message: &proto::LspExtRunnables) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_runnable_deserializes_as_shell() {
        let json = serde_json::json!({
            "label": "test my_test",
            "kind": "shell",
            "args": {
                "environment": {"RUSTC_TOOLCHAIN": "/path/to/toolchain"},
                "cwd": "/project",
                "program": "cargo",
                "args": ["nextest", "run", "--package", "my-crate", "--lib", "--", "my_test", "--exact", "--include-ignored"]
            }
        });

        let runnable: Runnable =
            serde_json::from_value(json).expect("shell runnable should deserialize");
        let RunnableArgs::Shell(shell) = &runnable.args else {
            panic!("expected Shell variant, got {:?}", runnable.args);
        };
        assert_eq!(shell.program, "cargo");
        assert_eq!(shell.args[0], "nextest");
        assert_eq!(shell.args[1], "run");
    }

    #[test]
    fn cargo_runnable_deserializes_as_cargo() {
        let json = serde_json::json!({
            "label": "cargo test -p my-crate",
            "kind": "cargo",
            "args": {
                "environment": {},
                "cwd": "/project",
                "overrideCargo": null,
                "workspaceRoot": "/project",
                "cargoArgs": ["test", "--package", "my-crate", "--lib"],
                "executableArgs": ["my_test", "--exact"]
            }
        });

        let runnable: Runnable =
            serde_json::from_value(json).expect("cargo runnable should deserialize");
        let RunnableArgs::Cargo(cargo) = &runnable.args else {
            panic!("expected Cargo variant, got {:?}", runnable.args);
        };
        assert_eq!(
            cargo.cargo_args,
            vec!["test", "--package", "my-crate", "--lib"]
        );
        assert_eq!(cargo.executable_args, vec!["my_test", "--exact"]);
    }
}
