use std::{collections::HashMap, fmt::Display};

use serde::{Deserialize, Serialize};

use super::LifecycleScript;
use super::deserializers::{
    deserialize_app_port, deserialize_mount_definition, deserialize_mount_definitions,
    deserialize_string_or_array,
};

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub(crate) enum ForwardPort {
    Number(u16),
    String(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PortAttributeProtocol {
    Https,
    Http,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum OnAutoForward {
    #[default]
    Notify,
    OpenBrowser,
    OpenBrowserOnce,
    OpenPreview,
    Silent,
    Ignore,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortAttributes {
    #[serde(default)]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) on_auto_forward: OnAutoForward,
    #[serde(default)]
    pub(crate) elevate_if_needed: bool,
    #[serde(default)]
    pub(crate) require_local_port: bool,
    #[serde(default)]
    pub(crate) protocol: Option<PortAttributeProtocol>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum UserEnvProbe {
    None,
    InteractiveShell,
    LoginShell,
    LoginInteractiveShell,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ShutdownAction {
    None,
    StopContainer,
    StopCompose,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MountDefinition {
    #[serde(default)]
    pub(crate) source: Option<String>,
    pub(crate) target: String,
    #[serde(rename = "type")]
    pub(crate) mount_type: Option<String>,
}

impl Display for MountDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mount_type = self.mount_type.clone().unwrap_or_else(|| {
            if let Some(source) = &self.source {
                if source.starts_with('/')
                    || source.starts_with("\\\\")
                    || source.get(1..3) == Some(":\\")
                    || source.get(1..3) == Some(":/")
                {
                    return "bind".to_string();
                }
            }
            "volume".to_string()
        });
        write!(f, "type={}", mount_type)?;
        if let Some(source) = &self.source {
            write!(f, ",source={}", source)?;
        }
        write!(f, ",target={},consistency=cached", self.target)
    }
}

/// Represents the value associated with a feature ID in the `features` map of devcontainer.json.
///
/// Per the spec, the value can be:
/// - A boolean (`true` to enable with defaults)
/// - A string (shorthand for `{"version": "<value>"}`)
/// - An object mapping option names to string or boolean values
///
/// See: https://containers.dev/implementors/features/#devcontainerjson-properties
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub(crate) enum FeatureOptions {
    Bool(bool),
    String(String),
    Options(HashMap<String, FeatureOptionValue>),
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub(crate) enum FeatureOptionValue {
    Bool(bool),
    String(String),
}
impl std::fmt::Display for FeatureOptionValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeatureOptionValue::Bool(b) => write!(f, "{}", b),
            FeatureOptionValue::String(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq, Default)]
pub(crate) struct MavCustomizationsWrapper {
    pub(crate) mav: MavCustomization,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
pub(crate) struct MavCustomization {
    #[serde(default)]
    pub(crate) extensions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContainerBuild {
    pub(crate) dockerfile: String,
    pub(crate) context: Option<String>,
    pub(crate) args: Option<HashMap<String, String>>,
    pub(crate) options: Option<Vec<String>>,
    pub(crate) target: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_array")]
    pub(crate) cache_from: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostRequirements {
    pub(crate) cpus: Option<u16>,
    pub(crate) memory: Option<String>,
    pub(crate) storage: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LifecycleCommand {
    InitializeCommand,
    OnCreateCommand,
    UpdateContentCommand,
    PostCreateCommand,
    PostStartCommand,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DevContainerBuildType {
    Image(String),
    Dockerfile(ContainerBuild),
    DockerCompose,
    None,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DevContainer {
    pub(crate) image: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) remote_user: Option<String>,
    pub(crate) forward_ports: Option<Vec<ForwardPort>>,
    pub(crate) ports_attributes: Option<HashMap<String, PortAttributes>>,
    pub(crate) other_ports_attributes: Option<PortAttributes>,
    pub(crate) container_env: Option<HashMap<String, String>>,
    pub(crate) remote_env: Option<HashMap<String, String>>,
    pub(crate) container_user: Option<String>,
    #[serde(rename = "updateRemoteUserUID")]
    pub(crate) update_remote_user_uid: Option<bool>,
    pub(crate) user_env_probe: Option<UserEnvProbe>,
    pub(crate) override_command: Option<bool>,
    pub(crate) shutdown_action: Option<ShutdownAction>,
    pub(crate) init: Option<bool>,
    pub(crate) privileged: Option<bool>,
    pub(crate) cap_add: Option<Vec<String>>,
    pub(crate) security_opt: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_mount_definitions")]
    pub(crate) mounts: Option<Vec<MountDefinition>>,
    pub(crate) features: Option<HashMap<String, FeatureOptions>>,
    pub(crate) override_feature_install_order: Option<Vec<String>>,
    pub(crate) customizations: Option<MavCustomizationsWrapper>,
    pub(crate) build: Option<ContainerBuild>,
    #[serde(default, deserialize_with = "deserialize_app_port")]
    pub(crate) app_port: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_mount_definition")]
    pub(crate) workspace_mount: Option<MountDefinition>,
    pub(crate) workspace_folder: Option<String>,
    pub(crate) run_args: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_string_or_array")]
    pub(crate) docker_compose_file: Option<Vec<String>>,
    pub(crate) service: Option<String>,
    pub(crate) run_services: Option<Vec<String>>,
    pub(crate) initialize_command: Option<LifecycleScript>,
    pub(crate) on_create_command: Option<LifecycleScript>,
    pub(crate) update_content_command: Option<LifecycleScript>,
    pub(crate) post_create_command: Option<LifecycleScript>,
    pub(crate) post_start_command: Option<LifecycleScript>,
    pub(crate) post_attach_command: Option<LifecycleScript>,
    pub(crate) wait_for: Option<LifecycleCommand>,
    pub(crate) host_requirements: Option<HostRequirements>,
}
