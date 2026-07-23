use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    compose_deserialize::{
        deserialize_compose_top_level_volumes, deserialize_compose_volumes, deserialize_labels,
        deserialize_nullable_vec,
    },
    metadata::{deserialize_metadata, deserialize_nullable_labels},
};
use crate::{devcontainer_api::DevContainerError, devcontainer_json::MountDefinition};

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct DockerPs {
    #[serde(alias = "ID")]
    pub(crate) id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct DockerState {
    pub(crate) running: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct DockerInspect {
    pub(crate) id: String,
    pub(crate) config: DockerInspectConfig,
    pub(crate) mounts: Option<Vec<DockerInspectMount>>,
    pub(crate) state: Option<DockerState>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerConfigLabels {
    #[serde(
        default,
        rename = "devcontainer.metadata",
        deserialize_with = "deserialize_metadata"
    )]
    pub(crate) metadata: Option<Vec<HashMap<String, serde_json_lenient::Value>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct DockerInspectConfig {
    #[serde(default, deserialize_with = "deserialize_nullable_labels")]
    pub(crate) labels: DockerConfigLabels,
    #[serde(rename = "User")]
    pub(crate) image_user: Option<String>,
    #[serde(default)]
    pub(crate) env: Vec<String>,
}

impl DockerInspectConfig {
    pub(crate) fn env_as_map(&self) -> Result<HashMap<String, String>, DevContainerError> {
        let mut map = HashMap::new();
        for env_var in &self.env {
            let Some((key, value)) = env_var.split_once('=') else {
                log::warn!("Skipping environment variable without a value: {env_var}");
                continue;
            };
            map.insert(key.to_string(), value.to_string());
        }
        Ok(map)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct DockerInspectMount {
    pub(crate) source: String,
    pub(crate) destination: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeServiceBuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dockerfile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) args: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) additional_contexts: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeServicePort {
    #[serde(deserialize_with = "deserialize_string_or_int")]
    pub(crate) target: String,
    #[serde(deserialize_with = "deserialize_string_or_int")]
    pub(crate) published: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) host_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) app_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

fn deserialize_string_or_int<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrInt {
        String(String),
        Int(u32),
    }

    match StringOrInt::deserialize(deserializer)? {
        StringOrInt::String(s) => Ok(s),
        StringOrInt::Int(b) => Ok(b.to_string()),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeService {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entrypoint: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cap_add: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) security_opt: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "deserialize_labels"
    )]
    pub(crate) labels: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) build: Option<DockerComposeServiceBuild>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) privileged: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_compose_volumes"
    )]
    pub(crate) volumes: Vec<MountDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) env_file: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) ports: Vec<DockerComposeServicePort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) network_mode: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_nullable_vec"
    )]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeVolume {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    pub(crate) services: HashMap<String, DockerComposeService>,
    #[serde(default, deserialize_with = "deserialize_compose_top_level_volumes")]
    pub(crate) volumes: HashMap<String, DockerComposeVolume>,
}

pub(crate) struct Docker {
    pub(super) docker_cli: String,
    pub(super) has_buildx: bool,
}

impl DockerInspect {
    pub(crate) fn is_running(&self) -> bool {
        self.state.as_ref().map_or(false, |s| s.running)
    }
}
