#[path = "devcontainer_manifest/compose_build.rs"]
mod compose_build;
#[path = "devcontainer_manifest/docker_build.rs"]
mod docker_build;
#[path = "devcontainer_manifest/docker_run.rs"]
mod docker_run;
#[path = "devcontainer_manifest/features.rs"]
mod features;
#[path = "devcontainer_manifest/helpers.rs"]
mod helpers;
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
    async fn new(
        context: &DevContainerContext,
        environment: HashMap<String, String>,
        docker_client: Arc<dyn DockerClient>,
        command_runner: Arc<dyn CommandRunner>,
        local_config: DevContainerConfig,
        local_project_path: &Path,
    ) -> Result<Self, DevContainerError> {
        let config_path = local_project_path.join(local_config.config_path.clone());
        log::debug!("parsing devcontainer json found in {:?}", &config_path);
        let devcontainer_contents = context.fs.load(&config_path).await.map_err(|e| {
            log::error!("Unable to read devcontainer contents: {e}");
            DevContainerError::DevContainerParseFailed
        })?;

        let devcontainer = deserialize_devcontainer_json(&devcontainer_contents)?;

        let devcontainer_directory = config_path.parent().ok_or_else(|| {
            log::error!("Dev container file should be in a directory");
            DevContainerError::NotInValidProject
        })?;
        let file_name = config_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| {
                log::error!("Dev container file has no file name, or is invalid unicode");
                DevContainerError::DevContainerParseFailed
            })?;

        Ok(Self {
            fs: context.fs.clone(),
            http_client: context.http_client.clone(),
            docker_client,
            command_runner,
            raw_config: devcontainer_contents,
            config: ConfigStatus::Deserialized(devcontainer),
            local_project_directory: local_project_path.to_path_buf(),
            local_environment: environment,
            config_directory: devcontainer_directory.to_path_buf(),
            file_name: file_name.to_string(),
            root_image: None,
            features_build_info: None,
            features: Vec::new(),
        })
    }

    fn devcontainer_id(&self) -> String {
        let mut labels = self.identifying_labels();
        labels.sort_by_key(|(key, _)| *key);

        let mut hasher = DefaultHasher::new();
        for (key, value) in &labels {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }

    fn identifying_labels(&self) -> Vec<(&str, String)> {
        let labels = vec![
            (
                "devcontainer.local_folder",
                (self.local_project_directory.display()).to_string(),
            ),
            (
                "devcontainer.config_file",
                (self.config_file().display()).to_string(),
            ),
        ];
        labels
    }

    fn parse_nonremote_vars_for_content(
        &self,
        content: &str,
    ) -> Result<serde_json_lenient::Value, DevContainerError> {
        let mut value = deserialize_devcontainer_json_to_value(content)?;
        let mut to_visit = vec![&mut value];

        while let Some(value) = to_visit.pop() {
            use serde_json_lenient::Value;

            match value {
                Value::String(string) => {
                    *string = string
                        .replace("${devcontainerId}", &self.devcontainer_id())
                        .replace(
                            "${containerWorkspaceFolderBasename}",
                            &self.remote_workspace_base_name().unwrap_or_default(),
                        )
                        .replace(
                            "${localWorkspaceFolderBasename}",
                            &self.local_workspace_base_name()?,
                        )
                        .replace(
                            "${containerWorkspaceFolder}",
                            &self
                                .remote_workspace_folder()
                                .map(|path| path.display().to_string())
                                .unwrap_or_default()
                                .replace('\\', "/"),
                        )
                        .replace(
                            "${localWorkspaceFolder}",
                            &self.local_workspace_folder().replace('\\', "/"),
                        );
                    *string = Self::replace_environment_variables(
                        string,
                        "localEnv",
                        &self.local_environment,
                    );
                }

                Value::Array(array) => to_visit.extend(array.iter_mut()),
                Value::Object(object) => to_visit.extend(object.values_mut()),

                Value::Null | Value::Bool(_) | Value::Number(_) => {}
            }
        }

        Ok(value)
    }

    fn parse_nonremote_vars(&mut self) -> Result<(), DevContainerError> {
        let replaced_content = self.parse_nonremote_vars_for_content(&self.raw_config)?;
        let parsed_config = deserialize_devcontainer_json_from_value(replaced_content)?;

        self.config = ConfigStatus::VariableParsed(parsed_config);

        Ok(())
    }

    fn runtime_remote_env(
        &self,
        container_env: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>, DevContainerError> {
        let mut merged_remote_env = container_env.clone();
        // HOME is user-specific, and we will often not run as the image user
        merged_remote_env.remove("HOME");
        if let Some(mut remote_env) = self.dev_container().remote_env.clone() {
            remote_env.values_mut().for_each(|value| {
                *value = Self::replace_environment_variables(value, "containerEnv", &container_env)
            });
            for (k, v) in remote_env {
                merged_remote_env.insert(k, v);
            }
        }
        Ok(merged_remote_env)
    }

    fn replace_environment_variables(
        mut orig: &str,
        environment_source: &str,
        environment: &HashMap<String, String>,
    ) -> String {
        let mut replaced = String::with_capacity(orig.len());
        let prefix = format!("${{{environment_source}:");
        while let Some(start) = orig.find(&prefix) {
            let var_name_start = start + prefix.len();
            let Some(end) = orig[var_name_start..].find('}') else {
                // No closing `}` => malformed variable reference => paste as is.
                break;
            };
            let end = var_name_start + end;

            let (var_name_end, default_start) =
                if let Some(var_name_end) = orig[var_name_start..end].find(':') {
                    let var_name_end = var_name_start + var_name_end;
                    (var_name_end, var_name_end + 1)
                } else {
                    (end, end)
                };

            let var_name = &orig[var_name_start..var_name_end];
            if var_name.is_empty() {
                // Empty variable name => paste as is.
                replaced.push_str(&orig[..end + 1]);
                orig = &orig[end + 1..];
                continue;
            }
            let default = &orig[default_start..end];

            replaced.push_str(&orig[..start]);
            replaced.push_str(
                environment
                    .get(var_name)
                    .map(|value| value.as_str())
                    .unwrap_or(default),
            );
            orig = &orig[end + 1..];
        }
        replaced.push_str(orig);
        replaced
    }

    fn config_file(&self) -> PathBuf {
        self.config_directory.join(&self.file_name)
    }

    fn dev_container(&self) -> &DevContainer {
        match &self.config {
            ConfigStatus::Deserialized(dev_container) => dev_container,
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        }
    }

    async fn dockerfile_location(&self) -> Option<PathBuf> {
        let dev_container = self.dev_container();
        match dev_container.build_type() {
            DevContainerBuildType::Image(_) => None,
            DevContainerBuildType::Dockerfile(build) => {
                Some(self.config_directory.join(&build.dockerfile))
            }
            DevContainerBuildType::DockerCompose => {
                let Ok(docker_compose_manifest) = self.docker_compose_manifest().await else {
                    return None;
                };
                let Ok((_, main_service)) = find_primary_service(&docker_compose_manifest, self)
                else {
                    return None;
                };
                main_service.build.and_then(|b| {
                    let compose_file = docker_compose_manifest.files.first()?;
                    resolve_compose_dockerfile(
                        compose_file,
                        b.context.as_deref(),
                        b.dockerfile.as_deref()?,
                    )
                })
            }
            DevContainerBuildType::None => None,
        }
    }

    fn generate_features_image_tag(&self, dockerfile_build_path: String) -> String {
        let mut hasher = DefaultHasher::new();
        let prefix = match &self.dev_container().name {
            Some(name) => &safe_id_lower(name),
            None => "mav-dc",
        };
        let prefix = prefix.get(..6).unwrap_or(prefix);
        let prefix = prefix.trim_matches(|c: char| !c.is_alphanumeric());

        dockerfile_build_path.hash(&mut hasher);

        let hash = hasher.finish();
        format!("{}-{:x}-features", prefix, hash)
    }

    /// Gets the base image from the devcontainer with the following precedence:
    /// - The devcontainer image if an image is specified
    /// - The image sourced in the Dockerfile if a Dockerfile is specified
    /// - The image sourced in the docker-compose main service, if one is specified
    /// - The image sourced in the docker-compose main service dockerfile, if one is specified
    /// If no such image is available, return an error
    async fn get_base_image_from_config(&self) -> Result<String, DevContainerError> {
        match self.dev_container().build_type() {
            DevContainerBuildType::Image(image) => {
                return Ok(image);
            }
            DevContainerBuildType::Dockerfile(build) => {
                let dockerfile_contents = self.expanded_dockerfile_content().await?;
                return image_from_dockerfile(dockerfile_contents, &build.target).ok_or_else(
                    || {
                        log::error!("Unable to find base image in Dockerfile");
                        DevContainerError::DevContainerParseFailed
                    },
                );
            }
            DevContainerBuildType::DockerCompose => {
                let docker_compose_manifest = self.docker_compose_manifest().await?;
                let (_, main_service) = find_primary_service(&docker_compose_manifest, &self)?;

                if let Some(_) = main_service
                    .build
                    .as_ref()
                    .and_then(|b| b.dockerfile.as_ref())
                {
                    let dockerfile_contents = self.expanded_dockerfile_content().await?;
                    return image_from_dockerfile(
                        dockerfile_contents,
                        &main_service.build.as_ref().and_then(|b| b.target.clone()),
                    )
                    .ok_or_else(|| {
                        log::error!("Unable to find base image in Dockerfile");
                        DevContainerError::DevContainerParseFailed
                    });
                }
                if let Some(image) = &main_service.image {
                    return Ok(image.to_string());
                }

                log::error!("No valid base image found in docker-compose configuration");
                return Err(DevContainerError::DevContainerParseFailed);
            }
            DevContainerBuildType::None => {
                log::error!("Not a valid devcontainer config for build");
                return Err(DevContainerError::NotInValidProject);
            }
        }
    }

    fn generate_dockerfile_extended(
        &self,
        container_user: &str,
        remote_user: &str,
        dockerfile_content: String,
        use_buildkit: bool,
    ) -> String {
        #[cfg(not(target_os = "windows"))]
        let update_remote_user_uid = self.dev_container().update_remote_user_uid.unwrap_or(true);
        #[cfg(target_os = "windows")]
        let update_remote_user_uid = false;
        let feature_layers: String = self
            .features
            .iter()
            .map(|manifest| {
                manifest.generate_dockerfile_feature_layer(
                    use_buildkit,
                    FEATURES_CONTAINER_TEMP_DEST_FOLDER,
                )
            })
            .collect();

        let container_home_cmd = get_ent_passwd_shell_command(container_user);
        let remote_home_cmd = get_ent_passwd_shell_command(remote_user);

        let dest = FEATURES_CONTAINER_TEMP_DEST_FOLDER;

        let feature_content_source_stage = if use_buildkit {
            "".to_string()
        } else {
            "\nFROM dev_container_feature_content_temp as dev_containers_feature_content_source\n"
                .to_string()
        };

        let builtin_env_source_path = if use_buildkit {
            "./devcontainer-features.builtin.env"
        } else {
            "/tmp/build-features/devcontainer-features.builtin.env"
        };

        let mut extended_dockerfile = format!(
            r#"ARG _DEV_CONTAINERS_BASE_IMAGE=placeholder

{dockerfile_content}
{feature_content_source_stage}
FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_feature_content_normalize
USER root
COPY --from=dev_containers_feature_content_source {builtin_env_source_path} /tmp/build-features/
RUN chmod -R 0755 /tmp/build-features/

FROM $_DEV_CONTAINERS_BASE_IMAGE AS dev_containers_target_stage

USER root

RUN mkdir -p {dest}
COPY --from=dev_containers_feature_content_normalize /tmp/build-features/ {dest}

RUN \
echo "_CONTAINER_USER_HOME=$({container_home_cmd} | cut -d: -f6)" >> {dest}/devcontainer-features.builtin.env && \
echo "_REMOTE_USER_HOME=$({remote_home_cmd} | cut -d: -f6)" >> {dest}/devcontainer-features.builtin.env

{feature_layers}

ARG _DEV_CONTAINERS_IMAGE_USER=root
USER $_DEV_CONTAINERS_IMAGE_USER
"#
        );

        // If we're not adding a uid update layer, then we should add env vars to this layer instead
        if !update_remote_user_uid {
            extended_dockerfile = format!(
                r#"{extended_dockerfile}
# Ensure that /etc/profile does not clobber the existing path
RUN sed -i -E 's/((^|\s)PATH=)([^\$]*)$/\1\${{PATH:-\3}}/g' /etc/profile || true
"#
            );

            for feature in &self.features {
                let container_env_layer = feature.generate_dockerfile_env();
                extended_dockerfile = format!("{extended_dockerfile}\n{container_env_layer}");
            }

            if let Some(env) = &self.dev_container().container_env {
                for (key, value) in env {
                    extended_dockerfile = format!("{extended_dockerfile}ENV {key}={value}\n");
                }
            }
        }

        extended_dockerfile
    }

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

    fn extension_ids(&self) -> Vec<String> {
        self.dev_container()
            .customizations
            .as_ref()
            .map(|c| c.mav.extensions.clone())
            .unwrap_or_default()
    }

    async fn build_and_run(&mut self) -> Result<DevContainerUp, DevContainerError> {
        self.dev_container().validate_devcontainer_contents()?;

        self.run_initialize_commands().await?;

        self.download_feature_and_dockerfile_resources().await?;

        let build_resources = self.build_resources().await?;

        let devcontainer_up = self.run_dev_container(build_resources).await?;

        self.run_remote_scripts(&devcontainer_up, true).await?;

        Ok(devcontainer_up)
    }

    async fn run_remote_scripts(
        &self,
        devcontainer_up: &DevContainerUp,
        new_container: bool,
    ) -> Result<(), DevContainerError> {
        let ConfigStatus::VariableParsed(config) = &self.config else {
            log::error!("Config not yet parsed, cannot proceed with remote scripts");
            return Err(DevContainerError::DevContainerScriptsFailed);
        };
        let remote_folder = self.remote_workspace_folder()?.display().to_string();

        if new_container {
            if let Some(on_create_command) = &config.on_create_command {
                for (command_name, command) in on_create_command.script_commands() {
                    log::debug!("Running on create command {command_name}");
                    self.docker_client
                        .run_docker_exec(
                            &devcontainer_up.container_id,
                            &remote_folder,
                            &devcontainer_up.remote_user,
                            &devcontainer_up.remote_env,
                            command,
                        )
                        .await?;
                }
            }
            if let Some(update_content_command) = &config.update_content_command {
                for (command_name, command) in update_content_command.script_commands() {
                    log::debug!("Running update content command {command_name}");
                    self.docker_client
                        .run_docker_exec(
                            &devcontainer_up.container_id,
                            &remote_folder,
                            &devcontainer_up.remote_user,
                            &devcontainer_up.remote_env,
                            command,
                        )
                        .await?;
                }
            }

            if let Some(post_create_command) = &config.post_create_command {
                for (command_name, command) in post_create_command.script_commands() {
                    log::debug!("Running post create command {command_name}");
                    self.docker_client
                        .run_docker_exec(
                            &devcontainer_up.container_id,
                            &remote_folder,
                            &devcontainer_up.remote_user,
                            &devcontainer_up.remote_env,
                            command,
                        )
                        .await?;
                }
            }
            if let Some(post_start_command) = &config.post_start_command {
                for (command_name, command) in post_start_command.script_commands() {
                    log::debug!("Running post start command {command_name}");
                    self.docker_client
                        .run_docker_exec(
                            &devcontainer_up.container_id,
                            &remote_folder,
                            &devcontainer_up.remote_user,
                            &devcontainer_up.remote_env,
                            command,
                        )
                        .await?;
                }
            }
        }
        if let Some(post_attach_command) = &config.post_attach_command {
            for (command_name, command) in post_attach_command.script_commands() {
                log::debug!("Running post attach command {command_name}");
                self.docker_client
                    .run_docker_exec(
                        &devcontainer_up.container_id,
                        &remote_folder,
                        &devcontainer_up.remote_user,
                        &devcontainer_up.remote_env,
                        command,
                    )
                    .await?;
            }
        }

        Ok(())
    }

    async fn run_initialize_commands(&self) -> Result<(), DevContainerError> {
        let ConfigStatus::VariableParsed(config) = &self.config else {
            log::error!("Config not yet parsed, cannot proceed with initializeCommand");
            return Err(DevContainerError::DevContainerParseFailed);
        };

        if let Some(initialize_command) = &config.initialize_command {
            log::debug!("Running initialize command");
            initialize_command
                .run(&self.command_runner, &self.local_project_directory)
                .await
        } else {
            log::warn!("No initialize command found");
            Ok(())
        }
    }

    async fn check_for_existing_devcontainer(
        &self,
    ) -> Result<Option<DevContainerUp>, DevContainerError> {
        if let Some(docker_ps) = self.check_for_existing_container().await? {
            log::debug!("Dev container already found. Proceeding with it");

            let docker_inspect = self.docker_client.inspect(&docker_ps.id).await?;

            if !docker_inspect.is_running() {
                log::debug!("Container not running. Will attempt to start, and then proceed");
                self.docker_client.start_container(&docker_ps.id).await?;
            }

            let remote_user = get_remote_user_from_config(&docker_inspect, self)?;

            let remote_folder = self.remote_workspace_folder()?;

            let remote_env = self.runtime_remote_env(&docker_inspect.config.env_as_map()?)?;

            let dev_container_up = DevContainerUp {
                container_id: docker_ps.id,
                remote_user: remote_user,
                remote_workspace_folder: remote_folder.display().to_string(),
                extension_ids: self.extension_ids(),
                remote_env,
            };

            self.run_remote_scripts(&dev_container_up, false).await?;

            Ok(Some(dev_container_up))
        } else {
            log::debug!("Existing container not found.");

            Ok(None)
        }
    }

    async fn check_for_existing_container(&self) -> Result<Option<DockerPs>, DevContainerError> {
        self.docker_client
            .find_process_by_filters(
                self.identifying_labels()
                    .iter()
                    .map(|(k, v)| format!("label={k}={v}"))
                    .collect(),
            )
            .await
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
