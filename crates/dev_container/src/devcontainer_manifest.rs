#[path = "devcontainer_manifest/compose_build.rs"]
mod compose_build;
#[path = "devcontainer_manifest/config_state.rs"]
mod config_state;
#[path = "devcontainer_manifest/docker_build.rs"]
mod docker_build;
#[path = "devcontainer_manifest/docker_run.rs"]
mod docker_run;
#[path = "devcontainer_manifest/features.rs"]
mod features;
#[path = "devcontainer_manifest/helpers.rs"]
mod helpers;
#[path = "devcontainer_manifest/image_config.rs"]
mod image_config;
#[path = "devcontainer_manifest/lifecycle.rs"]
mod lifecycle;
#[path = "devcontainer_manifest/path_derivation.rs"]
mod path_derivation;
#[cfg(test)]
#[path = "devcontainer_manifest/test_support.rs"]
mod test_support;

use std::{
    collections::HashMap,
    fmt::Debug,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

use regex::Regex;

use fs::Fs;
use http_client::HttpClient;
use util::{ResultExt, command::Command, normalize_path};

use helpers::*;

use crate::{
    DevContainerConfig, DevContainerContext,
    command_json::{CommandRunner, DefaultCommandRunner},
    devcontainer_api::{DevContainerError, DevContainerUp},
    devcontainer_json::{
        ContainerBuild, DevContainer, DevContainerBuildType, FeatureOptions, ForwardPort,
        MountDefinition, deserialize_devcontainer_json, deserialize_devcontainer_json_from_value,
        deserialize_devcontainer_json_to_value,
    },
    docker::{
        Docker, DockerClient, DockerComposeConfig, DockerComposeService, DockerComposeServiceBuild,
        DockerComposeServicePort, DockerComposeVolume, DockerInspect, DockerPs,
    },
    features::{DevContainerFeatureJson, FeatureManifest, parse_oci_feature_ref},
    get_oci_token,
    oci::{TokenResponse, download_oci_tarball, get_oci_manifest},
    safe_id_lower,
};

enum ConfigStatus {
    Deserialized(DevContainer),
    VariableParsed(DevContainer),
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub(crate) struct DockerComposeResources {
    files: Vec<PathBuf>,
    config: DockerComposeConfig,
}

struct DevContainerManifest {
    http_client: Arc<dyn HttpClient>,
    fs: Arc<dyn Fs>,
    docker_client: Arc<dyn DockerClient>,
    command_runner: Arc<dyn CommandRunner>,
    raw_config: String,
    config: ConfigStatus,
    local_environment: HashMap<String, String>,
    local_project_directory: PathBuf,
    config_directory: PathBuf,
    file_name: String,
    root_image: Option<DockerInspect>,
    features_build_info: Option<FeaturesBuildInfo>,
    features: Vec<FeatureManifest>,
}
const DEFAULT_REMOTE_PROJECT_DIR: &str = "/workspaces";
impl DevContainerManifest {
    fn build_merged_resources(
        &self,
        base_image: DockerInspect,
    ) -> Result<DockerBuildResources, DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet merge resources"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };
        let mut mounts = dev_container.mounts.clone().unwrap_or(Vec::new());

        let mut feature_mounts = self.features.iter().flat_map(|f| f.mounts()).collect();

        mounts.append(&mut feature_mounts);

        let privileged = dev_container.privileged.unwrap_or(false)
            || self.features.iter().any(|f| f.privileged());

        let entrypoint_script = if dev_container.override_command == Some(false) {
            None
        } else {
            let mut entrypoint_script_lines = vec![
                "echo Container started".to_string(),
                "trap \"exit 0\" 15".to_string(),
            ];

            for entrypoint in self.features.iter().filter_map(|f| f.entrypoint()) {
                entrypoint_script_lines.push(entrypoint.clone());
            }
            entrypoint_script_lines.append(&mut vec![
                "exec \"$@\"".to_string(),
                "while sleep 1 & wait $!; do :; done".to_string(),
            ]);

            Some(entrypoint_script_lines.join("\n").trim().to_string())
        };

        Ok(DockerBuildResources {
            image: base_image,
            additional_mounts: mounts,
            privileged,
            entrypoint_script,
        })
    }

    async fn build_resources(&self) -> Result<DevContainerBuildResources, DevContainerError> {
        if let ConfigStatus::Deserialized(_) = &self.config {
            log::error!(
                "Dev container has not yet been parsed for variable expansion. Cannot yet build resources"
            );
            return Err(DevContainerError::DevContainerParseFailed);
        }
        let dev_container = self.dev_container();
        match dev_container.build_type() {
            DevContainerBuildType::Image(base_image) => {
                let built_docker_image = self.build_docker_image().await?;

                let built_docker_image = self
                    .update_remote_user_uid(built_docker_image, &base_image)
                    .await?;

                let resources = self.build_merged_resources(built_docker_image)?;
                Ok(DevContainerBuildResources::Docker(resources))
            }
            DevContainerBuildType::Dockerfile(_) => {
                let built_docker_image = self.build_docker_image().await?;
                let Some(features_build_info) = &self.features_build_info else {
                    log::error!(
                        "Can't attempt to build update UID dockerfile before initial docker build"
                    );
                    return Err(DevContainerError::DevContainerParseFailed);
                };
                let built_docker_image = self
                    .update_remote_user_uid(built_docker_image, &features_build_info.image_tag)
                    .await?;

                let resources = self.build_merged_resources(built_docker_image)?;
                Ok(DevContainerBuildResources::Docker(resources))
            }
            DevContainerBuildType::DockerCompose => {
                log::debug!("Using docker compose. Building extended compose files");
                let docker_compose_resources = self.build_and_extend_compose_files().await?;

                return Ok(DevContainerBuildResources::DockerCompose(
                    docker_compose_resources,
                ));
            }
            DevContainerBuildType::None => {
                return Err(DevContainerError::DevContainerParseFailed);
            }
        }
    }

    async fn run_dev_container(
        &self,
        build_resources: DevContainerBuildResources,
    ) -> Result<DevContainerUp, DevContainerError> {
        let ConfigStatus::VariableParsed(_) = &self.config else {
            log::error!(
                "Variables have not been parsed; cannot proceed with running the dev container"
            );
            return Err(DevContainerError::DevContainerParseFailed);
        };
        let running_container = match build_resources {
            DevContainerBuildResources::DockerCompose(resources) => {
                self.run_docker_compose(resources).await?
            }
            DevContainerBuildResources::Docker(resources) => {
                self.run_docker_image(resources).await?
            }
        };

        let remote_user = get_remote_user_from_config(&running_container, self)?;
        let remote_workspace_folder = self.remote_workspace_folder()?;

        let remote_env = self.runtime_remote_env(&running_container.config.env_as_map()?)?;

        Ok(DevContainerUp {
            container_id: running_container.id,
            remote_user,
            remote_workspace_folder: remote_workspace_folder.display().to_string(),
            extension_ids: self.extension_ids(),
            remote_env,
        })
    }
}

/// Holds all the information needed to construct a `docker buildx build` command
/// that extends a base image with dev container features.
///
/// This mirrors the `ImageBuildOptions` interface in the CLI reference implementation
/// (cli/src/spec-node/containerFeatures.ts).
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct FeaturesBuildInfo {
    /// Path to the generated Dockerfile.extended
    pub dockerfile_path: PathBuf,
    /// Path to the features content directory (used as a BuildKit build context)
    pub features_content_dir: PathBuf,
    /// Path to an empty directory used as the Docker build context
    pub empty_context_dir: PathBuf,
    /// The base image name (e.g. "mcr.microsoft.com/devcontainers/rust:2-1-bookworm")
    pub build_image: Option<String>,
    /// The tag to apply to the built image (e.g. "vsc-myproject-features")
    pub image_tag: String,
}

pub(crate) async fn read_devcontainer_configuration(
    config: DevContainerConfig,
    context: &DevContainerContext,
    environment: HashMap<String, String>,
) -> Result<DevContainer, DevContainerError> {
    let docker = if context.use_podman {
        Docker::new("podman", context.use_buildkit).await
    } else {
        Docker::new("docker", context.use_buildkit).await
    };
    let mut dev_container = DevContainerManifest::new(
        context,
        environment,
        Arc::new(docker),
        Arc::new(DefaultCommandRunner::new()),
        config,
        &context.project_directory.as_ref(),
    )
    .await?;
    dev_container.parse_nonremote_vars()?;
    Ok(dev_container.dev_container().clone())
}

pub(crate) async fn spawn_dev_container(
    context: &DevContainerContext,
    environment: HashMap<String, String>,
    config: DevContainerConfig,
    local_project_path: &Path,
) -> Result<DevContainerUp, DevContainerError> {
    let docker = if context.use_podman {
        Docker::new("podman", context.use_buildkit).await
    } else {
        Docker::new("docker", context.use_buildkit).await
    };
    let mut devcontainer_manifest = DevContainerManifest::new(
        context,
        environment,
        Arc::new(docker),
        Arc::new(DefaultCommandRunner::new()),
        config,
        local_project_path,
    )
    .await?;

    devcontainer_manifest.parse_nonremote_vars()?;

    log::debug!("Checking for existing container");
    if let Some(devcontainer) = devcontainer_manifest
        .check_for_existing_devcontainer()
        .await?
    {
        Ok(devcontainer)
    } else {
        log::debug!("Existing container not found. Building");

        devcontainer_manifest.build_and_run().await
    }
}

#[derive(Debug)]
struct DockerBuildResources {
    image: DockerInspect,
    additional_mounts: Vec<MountDefinition>,
    privileged: bool,
    entrypoint_script: Option<String>,
}

#[derive(Debug)]
enum DevContainerBuildResources {
    DockerCompose(DockerComposeResources),
    Docker(DockerBuildResources),
}

#[cfg(test)]
mod test {
    use std::{
        collections::HashMap,
        ffi::OsStr,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use fs::{FakeFs, Fs};
    use gpui::{AppContext, TestAppContext};
    use http_client::HttpClient;
    use project::{
        ProjectEnvironment,
        worktree_store::{WorktreeIdCounter, WorktreeStore},
    };
    use util::paths::SanitizedPath;

    use crate::devcontainer_manifest::test_support::*;
    #[cfg(not(target_os = "windows"))]
    use crate::docker::DockerComposeServicePort;
    use crate::{
        DevContainerConfig, DevContainerContext,
        devcontainer_api::DevContainerError,
        devcontainer_json::MountDefinition,
        devcontainer_manifest::{
            ConfigStatus, DevContainerManifest, DockerBuildResources, DockerComposeResources,
            DockerInspect, extract_feature_id, find_primary_service, get_remote_user_from_config,
            image_from_dockerfile, is_local_feature_ref, resolve_compose_dockerfile,
        },
        docker::{
            DockerComposeConfig, DockerComposeService, DockerComposeServiceBuild,
            DockerConfigLabels, DockerInspectConfig,
        },
    };

    #[path = "compose_project_tests.rs"]
    mod compose_project_tests;

    #[path = "basic_tests.rs"]
    mod basic_tests;

    #[path = "dockerfile_feature_tests.rs"]
    mod dockerfile_feature_tests;

    #[path = "compose_spawn_tests.rs"]
    mod compose_spawn_tests;

    #[path = "compose_no_uid_tests.rs"]
    mod compose_no_uid_tests;

    #[path = "compose_services_tests.rs"]
    mod compose_services_tests;

    #[path = "compose_podman_tests.rs"]
    mod compose_podman_tests;

    #[path = "dockerfile_no_uid_tests.rs"]
    mod dockerfile_no_uid_tests;

    #[path = "local_feature_tests.rs"]
    mod local_feature_tests;

    #[path = "plain_image_tests.rs"]
    mod plain_image_tests;

    #[path = "dockerfile_parse_tests.rs"]
    mod dockerfile_parse_tests;

    #[path = "container_lookup_tests.rs"]
    mod container_lookup_tests;

    #[test]
    fn test_aliases_dockerfile_with_pre_existing_aliases_for_build() {}

    #[test]
    fn test_aliases_dockerfile_with_no_aliases_for_build() {}

    #[test]
    fn test_aliases_dockerfile_with_build_target_specified() {}
}
