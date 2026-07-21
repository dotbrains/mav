#[path = "devcontainer_manifest/docker_run.rs"]
mod docker_run;
#[path = "devcontainer_manifest/features.rs"]
mod features;
#[path = "devcontainer_manifest/helpers.rs"]
mod helpers;
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

    async fn docker_compose_manifest(&self) -> Result<DockerComposeResources, DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet get docker compose files"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };
        let Some(docker_compose_files) = dev_container.docker_compose_file.clone() else {
            return Err(DevContainerError::DevContainerParseFailed);
        };
        // Normalize upfront so every downstream consumer of
        // `DockerComposeResources.files` (compose fragment reads, project-name
        // derivation, `docker compose -f` invocations, …) sees resolved paths.
        // `dockerComposeFile` entries are joined verbatim with
        // `config_directory`, so raw entries can carry `..` components.
        let docker_compose_full_paths = docker_compose_files
            .iter()
            .map(|relative| normalize_path(&self.config_directory.join(relative)))
            .collect::<Vec<PathBuf>>();

        let Some(config) = self
            .docker_client
            .get_docker_compose_config(&docker_compose_full_paths)
            .await?
        else {
            log::error!("Output could not deserialize into DockerComposeConfig");
            return Err(DevContainerError::DevContainerParseFailed);
        };
        Ok(DockerComposeResources {
            files: docker_compose_full_paths,
            config,
        })
    }

    async fn build_and_extend_compose_files(
        &self,
    ) -> Result<DockerComposeResources, DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet build from compose files"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };

        let Some(features_build_info) = &self.features_build_info else {
            log::error!(
                "Cannot build and extend compose files: features build info is not yet constructed"
            );
            return Err(DevContainerError::DevContainerParseFailed);
        };
        let mut docker_compose_resources = self.docker_compose_manifest().await?;
        let supports_buildkit = self.docker_client.supports_compose_buildkit();

        let (main_service_name, main_service) =
            find_primary_service(&docker_compose_resources, self)?;
        let (built_service_image, built_service_image_tag) = if main_service
            .build
            .as_ref()
            .map(|b| b.dockerfile.as_ref())
            .is_some()
        {
            if !supports_buildkit {
                self.build_feature_content_image().await?;
            }

            let dockerfile_path = &features_build_info.dockerfile_path;

            let build_args = if !supports_buildkit {
                HashMap::from([
                    (
                        "_DEV_CONTAINERS_BASE_IMAGE".to_string(),
                        "dev_container_auto_added_stage_label".to_string(),
                    ),
                    ("_DEV_CONTAINERS_IMAGE_USER".to_string(), "root".to_string()),
                ])
            } else {
                HashMap::from([
                    ("BUILDKIT_INLINE_CACHE".to_string(), "1".to_string()),
                    (
                        "_DEV_CONTAINERS_BASE_IMAGE".to_string(),
                        "dev_container_auto_added_stage_label".to_string(),
                    ),
                    ("_DEV_CONTAINERS_IMAGE_USER".to_string(), "root".to_string()),
                ])
            };

            let additional_contexts = if !supports_buildkit {
                None
            } else {
                Some(HashMap::from([(
                    "dev_containers_feature_content_source".to_string(),
                    features_build_info
                        .features_content_dir
                        .display()
                        .to_string(),
                )]))
            };

            let build_override = DockerComposeConfig {
                name: None,
                services: HashMap::from([(
                    main_service_name.clone(),
                    DockerComposeService {
                        image: Some(features_build_info.image_tag.clone()),
                        entrypoint: None,
                        cap_add: None,
                        security_opt: None,
                        labels: None,
                        build: Some(DockerComposeServiceBuild {
                            context: Some(
                                main_service
                                    .build
                                    .as_ref()
                                    .and_then(|b| b.context.clone())
                                    .unwrap_or_else(|| {
                                        features_build_info.empty_context_dir.display().to_string()
                                    }),
                            ),
                            dockerfile: Some(dockerfile_path.display().to_string()),
                            target: Some("dev_containers_target_stage".to_string()),
                            args: Some(build_args),
                            additional_contexts,
                        }),
                        volumes: Vec::new(),
                        ..Default::default()
                    },
                )]),
                volumes: HashMap::new(),
            };

            let temp_base = std::env::temp_dir().join("devcontainer-mav");
            let config_location = temp_base.join("docker_compose_build.json");

            let config_json = serde_json_lenient::to_string(&build_override).map_err(|e| {
                log::error!("Error serializing docker compose runtime override: {e}");
                DevContainerError::DevContainerParseFailed
            })?;

            self.fs
                .write(&config_location, config_json.as_bytes())
                .await
                .map_err(|e| {
                    log::error!("Error writing the runtime override file: {e}");
                    DevContainerError::FilesystemError
                })?;

            docker_compose_resources.files.push(config_location);

            let project_name = self.project_name().await?;
            self.docker_client
                .docker_compose_build(
                    &docker_compose_resources.files,
                    &project_name,
                    dev_container.run_services.as_ref(),
                )
                .await?;
            (
                self.docker_client
                    .inspect(&features_build_info.image_tag)
                    .await?,
                &features_build_info.image_tag,
            )
        } else if let Some(image) = &main_service.image {
            if dev_container
                .features
                .as_ref()
                .is_none_or(|features| features.is_empty())
            {
                (self.docker_client.inspect(image).await?, image)
            } else {
                if !supports_buildkit {
                    self.build_feature_content_image().await?;
                }

                let dockerfile_path = &features_build_info.dockerfile_path;

                let build_args = if !supports_buildkit {
                    HashMap::from([
                        ("_DEV_CONTAINERS_BASE_IMAGE".to_string(), image.clone()),
                        ("_DEV_CONTAINERS_IMAGE_USER".to_string(), "root".to_string()),
                    ])
                } else {
                    HashMap::from([
                        ("BUILDKIT_INLINE_CACHE".to_string(), "1".to_string()),
                        ("_DEV_CONTAINERS_BASE_IMAGE".to_string(), image.clone()),
                        ("_DEV_CONTAINERS_IMAGE_USER".to_string(), "root".to_string()),
                    ])
                };

                let additional_contexts = if !supports_buildkit {
                    None
                } else {
                    Some(HashMap::from([(
                        "dev_containers_feature_content_source".to_string(),
                        features_build_info
                            .features_content_dir
                            .display()
                            .to_string(),
                    )]))
                };

                let build_override = DockerComposeConfig {
                    name: None,
                    services: HashMap::from([(
                        main_service_name.clone(),
                        DockerComposeService {
                            image: Some(features_build_info.image_tag.clone()),
                            entrypoint: None,
                            cap_add: None,
                            security_opt: None,
                            labels: None,
                            build: Some(DockerComposeServiceBuild {
                                context: Some(
                                    features_build_info.empty_context_dir.display().to_string(),
                                ),
                                dockerfile: Some(dockerfile_path.display().to_string()),
                                target: Some("dev_containers_target_stage".to_string()),
                                args: Some(build_args),
                                additional_contexts,
                            }),
                            volumes: Vec::new(),
                            ..Default::default()
                        },
                    )]),
                    volumes: HashMap::new(),
                };

                let temp_base = std::env::temp_dir().join("devcontainer-mav");
                let config_location = temp_base.join("docker_compose_build.json");

                let config_json = serde_json_lenient::to_string(&build_override).map_err(|e| {
                    log::error!("Error serializing docker compose runtime override: {e}");
                    DevContainerError::DevContainerParseFailed
                })?;

                self.fs
                    .write(&config_location, config_json.as_bytes())
                    .await
                    .map_err(|e| {
                        log::error!("Error writing the runtime override file: {e}");
                        DevContainerError::FilesystemError
                    })?;

                docker_compose_resources.files.push(config_location);

                let project_name = self.project_name().await?;
                self.docker_client
                    .docker_compose_build(
                        &docker_compose_resources.files,
                        &project_name,
                        dev_container.run_services.as_ref(),
                    )
                    .await?;

                (
                    self.docker_client
                        .inspect(&features_build_info.image_tag)
                        .await?,
                    &features_build_info.image_tag,
                )
            }
        } else {
            log::error!("Docker compose must have either image or dockerfile defined");
            return Err(DevContainerError::DevContainerParseFailed);
        };

        let built_service_image = self
            .update_remote_user_uid(built_service_image, built_service_image_tag)
            .await?;

        let resources = self.build_merged_resources(built_service_image)?;

        let network_mode = main_service.network_mode.as_ref();
        let network_mode_service = network_mode.and_then(|mode| mode.strip_prefix("service:"));
        let runtime_override_file = self
            .write_runtime_override_file(&main_service_name, network_mode_service, resources)
            .await?;

        docker_compose_resources.files.push(runtime_override_file);

        Ok(docker_compose_resources)
    }

    async fn write_runtime_override_file(
        &self,
        main_service_name: &str,
        network_mode_service: Option<&str>,
        resources: DockerBuildResources,
    ) -> Result<PathBuf, DevContainerError> {
        let config =
            self.build_runtime_override(main_service_name, network_mode_service, resources)?;
        let temp_base = std::env::temp_dir().join("devcontainer-mav");
        let config_location = temp_base.join("docker_compose_runtime.json");

        let config_json = serde_json_lenient::to_string(&config).map_err(|e| {
            log::error!("Error serializing docker compose runtime override: {e}");
            DevContainerError::DevContainerParseFailed
        })?;

        self.fs
            .write(&config_location, config_json.as_bytes())
            .await
            .map_err(|e| {
                log::error!("Error writing the runtime override file: {e}");
                DevContainerError::FilesystemError
            })?;

        Ok(config_location)
    }

    fn build_runtime_override(
        &self,
        main_service_name: &str,
        network_mode_service: Option<&str>,
        resources: DockerBuildResources,
    ) -> Result<DockerComposeConfig, DevContainerError> {
        let mut runtime_labels = HashMap::new();

        if let Some(metadata) = &resources.image.config.labels.metadata {
            let serialized_metadata = serde_json_lenient::to_string(metadata).map_err(|e| {
                log::error!("Error serializing docker image metadata: {e}");
                DevContainerError::ContainerNotValid(resources.image.id.clone())
            })?;

            runtime_labels.insert("devcontainer.metadata".to_string(), serialized_metadata);
        }

        for (k, v) in self.identifying_labels() {
            runtime_labels.insert(k.to_string(), v.to_string());
        }

        let config_volumes: HashMap<String, DockerComposeVolume> = resources
            .additional_mounts
            .iter()
            .filter_map(|mount| {
                if let Some(mount_type) = &mount.mount_type
                    && mount_type.to_lowercase() == "volume"
                    && let Some(source) = &mount.source
                {
                    Some((
                        source.clone(),
                        DockerComposeVolume {
                            name: Some(source.clone()),
                        },
                    ))
                } else {
                    None
                }
            })
            .collect();

        let volumes: Vec<MountDefinition> = resources
            .additional_mounts
            .iter()
            .map(|v| MountDefinition {
                source: v.source.clone(),
                target: v.target.clone(),
                mount_type: v.mount_type.clone(),
            })
            .collect();

        let entrypoint = resources.entrypoint_script.map(|script| {
            vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                script,
                "-".to_string(),
            ]
        });

        let mut main_service = DockerComposeService {
            entrypoint,
            cap_add: Some(vec!["SYS_PTRACE".to_string()]),
            security_opt: Some(vec!["seccomp=unconfined".to_string()]),
            labels: Some(runtime_labels),
            volumes,
            privileged: Some(resources.privileged),
            ..Default::default()
        };
        // let mut extra_service_port_declarations: Vec<(String, DockerComposeService)> = Vec::new();
        let mut service_declarations: HashMap<String, DockerComposeService> = HashMap::new();
        if let Some(forward_ports) = &self.dev_container().forward_ports {
            let main_service_ports: Vec<String> = forward_ports
                .iter()
                .filter_map(|f| match f {
                    ForwardPort::Number(port) => Some(port.to_string()),
                    ForwardPort::String(port) => {
                        let parts: Vec<&str> = port.split(":").collect();
                        if parts.len() <= 1 {
                            Some(port.to_string())
                        } else if parts.len() == 2 {
                            if parts[0] == main_service_name {
                                Some(parts[1].to_string())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                })
                .collect();
            for port in main_service_ports {
                // If the main service uses a different service's network bridge, append to that service's ports instead
                if let Some(network_service_name) = network_mode_service {
                    if let Some(service) = service_declarations.get_mut(network_service_name) {
                        service.ports.push(DockerComposeServicePort {
                            target: port.clone(),
                            published: port.clone(),
                            ..Default::default()
                        });
                    } else {
                        service_declarations.insert(
                            network_service_name.to_string(),
                            DockerComposeService {
                                ports: vec![DockerComposeServicePort {
                                    target: port.clone(),
                                    published: port.clone(),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            },
                        );
                    }
                } else {
                    main_service.ports.push(DockerComposeServicePort {
                        target: port.clone(),
                        published: port.clone(),
                        ..Default::default()
                    });
                }
            }
            let other_service_ports: Vec<(&str, &str)> = forward_ports
                .iter()
                .filter_map(|f| match f {
                    ForwardPort::Number(_) => None,
                    ForwardPort::String(port) => {
                        let parts: Vec<&str> = port.split(":").collect();
                        if parts.len() != 2 {
                            None
                        } else {
                            if parts[0] == main_service_name {
                                None
                            } else {
                                Some((parts[0], parts[1]))
                            }
                        }
                    }
                })
                .collect();
            for (service_name, port) in other_service_ports {
                if let Some(service) = service_declarations.get_mut(service_name) {
                    service.ports.push(DockerComposeServicePort {
                        target: port.to_string(),
                        published: port.to_string(),
                        ..Default::default()
                    });
                } else {
                    service_declarations.insert(
                        service_name.to_string(),
                        DockerComposeService {
                            ports: vec![DockerComposeServicePort {
                                target: port.to_string(),
                                published: port.to_string(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                    );
                }
            }
        }

        service_declarations.insert(main_service_name.to_string(), main_service);
        let new_docker_compose_config = DockerComposeConfig {
            name: None,
            services: service_declarations,
            volumes: config_volumes,
        };

        Ok(new_docker_compose_config)
    }

    async fn build_docker_image(&self) -> Result<DockerInspect, DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet build image"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };

        match dev_container.build_type() {
            DevContainerBuildType::Image(image_tag) => {
                let base_image = self.docker_client.inspect(&image_tag).await?;
                if dev_container
                    .features
                    .as_ref()
                    .is_none_or(|features| features.is_empty())
                {
                    log::debug!("No features to add. Using base image");
                    return Ok(base_image);
                }
            }
            DevContainerBuildType::Dockerfile(_) => {}
            DevContainerBuildType::DockerCompose | DevContainerBuildType::None => {
                return Err(DevContainerError::DevContainerParseFailed);
            }
        };

        let mut command = self.create_docker_build()?;

        let output = self
            .command_runner
            .run_command(&mut command)
            .await
            .map_err(|e| {
                log::error!("Error building docker image: {e}");
                DevContainerError::CommandFailed(command.get_program().display().to_string())
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("docker buildx build failed: {stderr}");
            return Err(DevContainerError::CommandFailed(
                command.get_program().display().to_string(),
            ));
        }

        // After a successful build, inspect the newly tagged image to get its metadata
        let Some(features_build_info) = &self.features_build_info else {
            log::error!("Features build info expected, but not created");
            return Err(DevContainerError::DevContainerParseFailed);
        };
        let image = self
            .docker_client
            .inspect(&features_build_info.image_tag)
            .await?;

        Ok(image)
    }

    #[cfg(target_os = "windows")]
    async fn update_remote_user_uid(
        &self,
        image: DockerInspect,
        _base_image: &str,
    ) -> Result<DockerInspect, DevContainerError> {
        Ok(image)
    }
    #[cfg(not(target_os = "windows"))]
    async fn update_remote_user_uid(
        &self,
        image: DockerInspect,
        base_image: &str,
    ) -> Result<DockerInspect, DevContainerError> {
        let dev_container = self.dev_container();

        let Some(features_build_info) = &self.features_build_info else {
            return Ok(image);
        };

        // updateRemoteUserUID defaults to true per the devcontainers spec
        if dev_container.update_remote_user_uid == Some(false) {
            return Ok(image);
        }

        let remote_user = get_remote_user_from_config(&image, self)?;
        if remote_user == "root" || remote_user.chars().all(|c| c.is_ascii_digit()) {
            return Ok(image);
        }

        let image_user = image
            .config
            .image_user
            .as_deref()
            .unwrap_or("root")
            .to_string();

        let host_uid = Command::new("id")
            .arg("-u")
            .output()
            .await
            .map_err(|e| {
                log::error!("Failed to get host UID: {e}");
                DevContainerError::CommandFailed("id -u".to_string())
            })
            .and_then(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| {
                        log::error!("Failed to parse host UID: {e}");
                        DevContainerError::CommandFailed("id -u".to_string())
                    })
            })?;

        let host_gid = Command::new("id")
            .arg("-g")
            .output()
            .await
            .map_err(|e| {
                log::error!("Failed to get host GID: {e}");
                DevContainerError::CommandFailed("id -g".to_string())
            })
            .and_then(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| {
                        log::error!("Failed to parse host GID: {e}");
                        DevContainerError::CommandFailed("id -g".to_string())
                    })
            })?;

        let dockerfile_content = self.generate_update_uid_dockerfile();

        let dockerfile_path = features_build_info
            .features_content_dir
            .join("updateUID.Dockerfile");
        self.fs
            .write(&dockerfile_path, dockerfile_content.as_bytes())
            .await
            .map_err(|e| {
                log::error!("Failed to write updateUID Dockerfile: {e}");
                DevContainerError::FilesystemError
            })?;

        let updated_image_tag = features_build_info.image_tag.clone();

        let mut command = Command::new(self.docker_client.docker_cli());
        // Without a usable BuildKit, force the classic builder: the build's
        // `FROM $BASE_IMAGE` references the locally-built features image, which
        // only resolves from the daemon's image store under the classic builder.
        if !self.docker_client.supports_compose_buildkit()
            && self.docker_client.docker_cli() != "podman"
        {
            command.env("DOCKER_BUILDKIT", "0");
        }
        command.args(["build"]);
        command.args(["-f", &dockerfile_path.display().to_string()]);
        command.args(["-t", &updated_image_tag]);
        command.args(["--build-arg", &format!("BASE_IMAGE={}", base_image)]);
        command.args(["--build-arg", &format!("REMOTE_USER={}", remote_user)]);
        command.args(["--build-arg", &format!("NEW_UID={}", host_uid)]);
        command.args(["--build-arg", &format!("NEW_GID={}", host_gid)]);
        command.args(["--build-arg", &format!("IMAGE_USER={}", image_user)]);
        command.arg(features_build_info.empty_context_dir.display().to_string());

        let output = self
            .command_runner
            .run_command(&mut command)
            .await
            .map_err(|e| {
                log::error!("Error building UID update image: {e}");
                DevContainerError::CommandFailed(command.get_program().display().to_string())
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("UID update build failed: {stderr}");
            return Err(DevContainerError::CommandFailed(
                command.get_program().display().to_string(),
            ));
        }

        self.docker_client.inspect(&updated_image_tag).await
    }

    #[cfg(not(target_os = "windows"))]
    fn generate_update_uid_dockerfile(&self) -> String {
        let mut dockerfile = r#"ARG BASE_IMAGE
FROM $BASE_IMAGE

USER root

ARG REMOTE_USER
ARG NEW_UID
ARG NEW_GID
SHELL ["/bin/sh", "-c"]
RUN eval $(sed -n "s/${REMOTE_USER}:[^:]*:\([^:]*\):\([^:]*\):[^:]*:\([^:]*\).*/OLD_UID=\1;OLD_GID=\2;HOME_FOLDER=\3/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_UID}:.*/EXISTING_USER=\1/p" /etc/passwd); \
	eval $(sed -n "s/\([^:]*\):[^:]*:${NEW_GID}:.*/EXISTING_GROUP=\1/p" /etc/group); \
	if [ -z "$OLD_UID" ]; then \
		echo "Remote user not found in /etc/passwd ($REMOTE_USER)."; \
	elif [ "$OLD_UID" = "$NEW_UID" -a "$OLD_GID" = "$NEW_GID" ]; then \
		echo "UIDs and GIDs are the same ($NEW_UID:$NEW_GID)."; \
	elif [ "$OLD_UID" != "$NEW_UID" -a -n "$EXISTING_USER" ]; then \
		echo "User with UID exists ($EXISTING_USER=$NEW_UID)."; \
	else \
		if [ "$OLD_GID" != "$NEW_GID" -a -n "$EXISTING_GROUP" ]; then \
			FREE_GID=65532; \
			while grep -q ":[^:]*:${FREE_GID}:" /etc/group; do FREE_GID=$((FREE_GID - 1)); done; \
			echo "Reassigning group $EXISTING_GROUP from GID $NEW_GID to $FREE_GID."; \
			sed -i -e "s/\(${EXISTING_GROUP}:[^:]*:\)${NEW_GID}:/\1${FREE_GID}:/" /etc/group; \
		fi; \
		echo "Updating UID:GID from $OLD_UID:$OLD_GID to $NEW_UID:$NEW_GID."; \
		sed -i -e "s/\(${REMOTE_USER}:[^:]*:\)[^:]*:[^:]*/\1${NEW_UID}:${NEW_GID}/" /etc/passwd; \
		if [ "$OLD_GID" != "$NEW_GID" ]; then \
			sed -i -e "s/\([^:]*:[^:]*:\)${OLD_GID}:/\1${NEW_GID}:/" /etc/group; \
		fi; \
		chown -R $NEW_UID:$NEW_GID $HOME_FOLDER; \
	fi;

ARG IMAGE_USER
USER $IMAGE_USER

# Ensure that /etc/profile does not clobber the existing path
RUN sed -i -E 's/((^|\s)PATH=)([^\$]*)$/\1\${PATH:-\3}/g' /etc/profile || true
"#.to_string();
        for feature in &self.features {
            let container_env_layer = feature.generate_dockerfile_env();
            dockerfile = format!("{dockerfile}\n{container_env_layer}");
        }

        if let Some(env) = &self.dev_container().container_env {
            for (key, value) in env {
                dockerfile = format!("{dockerfile}ENV {key}={value}\n");
            }
        }
        dockerfile
    }

    async fn build_feature_content_image(&self) -> Result<(), DevContainerError> {
        let Some(features_build_info) = &self.features_build_info else {
            log::error!("Features build info not available for building feature content image");
            return Err(DevContainerError::DevContainerParseFailed);
        };
        let features_content_dir = &features_build_info.features_content_dir;

        let dockerfile_content = "FROM scratch\nCOPY . /tmp/build-features/\n";
        let dockerfile_path = features_content_dir.join("Dockerfile.feature-content");

        self.fs
            .write(&dockerfile_path, dockerfile_content.as_bytes())
            .await
            .map_err(|e| {
                log::error!("Failed to write feature content Dockerfile: {e}");
                DevContainerError::FilesystemError
            })?;

        let mut command = Command::new(self.docker_client.docker_cli());
        // This path runs only when BuildKit is unavailable, so force the classic
        // builder: the feature content image is consumed by a later multi-stage
        // `FROM`, which requires it to live in the daemon's image store.
        if self.docker_client.docker_cli() != "podman" {
            command.env("DOCKER_BUILDKIT", "0");
        }
        command.args([
            "build",
            "-t",
            "dev_container_feature_content_temp",
            "-f",
            &dockerfile_path.display().to_string(),
            &features_content_dir.display().to_string(),
        ]);

        let output = self
            .command_runner
            .run_command(&mut command)
            .await
            .map_err(|e| {
                log::error!("Error building feature content image: {e}");
                DevContainerError::CommandFailed(self.docker_client.docker_cli())
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Feature content image build failed: {stderr}");
            return Err(DevContainerError::CommandFailed(
                self.docker_client.docker_cli(),
            ));
        }

        Ok(())
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

    /// Matches `@devcontainers/cli`'s `getProjectName` in
    /// `src/spec-node/dockerCompose.ts`. See `derive_project_name` for the
    /// full precedence. Using the devcontainer.json `name` field here
    /// diverges from the reference CLI and creates duplicate compose
    /// projects when the same folder is opened by both tools — see #54255.
    ///
    /// Async because the derivation reads both the workspace `.env` file
    /// and the merged compose config — neither of which is available
    /// synchronously.
    async fn project_name(&self) -> Result<String, DevContainerError> {
        let workspace_fallback = self
            .local_workspace_base_name()
            .unwrap_or_else(|_| self.local_workspace_folder());
        let compose_resources = self.docker_compose_manifest().await.ok();
        let first_compose_file = compose_resources
            .as_ref()
            .and_then(|r| r.files.first())
            .map(PathBuf::as_path);
        let compose_config_name = compose_resources
            .as_ref()
            .and_then(|r| r.config.name.as_deref());
        let mut compose_name_explicitly_declared = false;
        if let Some(resources) = &compose_resources {
            for file in &resources.files {
                // Mirrors the CLI's fragment re-parse (dockerCompose.ts 663-673):
                // the whole readFile+yaml.load pair is wrapped in a single
                // try/catch that swallows every failure. The comment there
                // calls out `!reset` custom tags; the behavior is "on any
                // failure, treat the fragment as not-declared and keep
                // scanning." Propagating an I/O error here would diverge
                // from that policy and fail the whole devcontainer flow for
                // a fragment the CLI would have silently skipped.
                let contents = match self.fs.load(file).await {
                    Ok(contents) => contents,
                    Err(err) => {
                        log::warn!(
                            "Ignoring unreadable compose fragment `{}` while deriving project name: {err:?}",
                            file.display()
                        );
                        continue;
                    }
                };
                if compose_fragment_declares_name(&contents) {
                    compose_name_explicitly_declared = true;
                    break;
                }
            }
        }
        let dotenv_path = self.local_project_directory.join(".env");
        let dotenv_contents = match self.fs.load(&dotenv_path).await {
            Ok(contents) => Some(contents),
            Err(err) if is_missing_file_error(&err) => None,
            Err(err) => {
                // Mirrors the CLI: `getProjectName` only swallows `ENOENT`/
                // `EISDIR` on the `.env` read. Any other error (permission
                // denied, I/O failure, …) must surface so we don't silently
                // fall back to a non-canonical project name and create a
                // second compose project for the same repo.
                log::error!(
                    "Failed to read workspace .env `{}` while deriving project name: {err:?}",
                    dotenv_path.display()
                );
                return Err(DevContainerError::FilesystemError);
            }
        };
        Ok(derive_project_name(
            &self.local_environment,
            dotenv_contents.as_deref(),
            compose_config_name,
            compose_name_explicitly_declared,
            first_compose_file,
            &self.local_project_directory,
            &workspace_fallback,
        ))
    }

    async fn expanded_dockerfile_content(&self) -> Result<String, DevContainerError> {
        let Some(dockerfile_path) = self.dockerfile_location().await else {
            log::error!("Tried to expand dockerfile for an image-type config");
            return Err(DevContainerError::DevContainerParseFailed);
        };

        // For docker-compose configs the build args live on the primary
        // compose service rather than on dev_container.build.
        let devcontainer_args = match self.dev_container().build_type() {
            DevContainerBuildType::DockerCompose => {
                let compose = self.docker_compose_manifest().await?;
                find_primary_service(&compose, self)?
                    .1
                    .build
                    .and_then(|b| b.args)
                    .unwrap_or_default()
            }
            _ => self
                .dev_container()
                .build
                .as_ref()
                .and_then(|b| b.args.clone())
                .unwrap_or_default(),
        };
        let contents = self.fs.load(&dockerfile_path).await.map_err(|e| {
            log::error!("Failed to load Dockerfile: {e}");
            DevContainerError::FilesystemError
        })?;
        let mut parsed_lines: Vec<String> = Vec::new();
        let mut inline_args: Vec<(String, String)> = Vec::new();
        let key_regex = Regex::new(r"(?:^|\s)(\w+)=").expect("valid regex");

        for line in contents.lines() {
            let mut parsed_line = line.to_string();
            // Replace from devcontainer args first, since they take precedence
            for (key, value) in &devcontainer_args {
                parsed_line = expand_dockerfile_var(parsed_line, key, value);
            }
            for (key, value) in &inline_args {
                parsed_line = expand_dockerfile_var(parsed_line, key, value);
            }
            if let Some(arg_directives) = parsed_line.strip_prefix("ARG ") {
                let trimmed = arg_directives.trim();
                let key_matches: Vec<_> = key_regex.captures_iter(trimmed).collect();
                for (i, captures) in key_matches.iter().enumerate() {
                    let key = captures[1].to_string();
                    // Insert the devcontainer overrides here if needed
                    let value_start = captures.get(0).expect("full match").end();
                    let value_end = if i + 1 < key_matches.len() {
                        key_matches[i + 1].get(0).expect("full match").start()
                    } else {
                        trimmed.len()
                    };
                    let raw_value = trimmed[value_start..value_end].trim();
                    let value = if raw_value.starts_with('"')
                        && raw_value.ends_with('"')
                        && raw_value.len() > 1
                    {
                        &raw_value[1..raw_value.len() - 1]
                    } else {
                        raw_value
                    };
                    inline_args.push((key, value.to_string()));
                }
            }
            parsed_lines.push(parsed_line);
        }

        Ok(parsed_lines.join("\n"))
    }

    fn calculate_context_dir(&self, build: ContainerBuild) -> PathBuf {
        let Some(context) = build.context else {
            return self.config_directory.clone();
        };
        let context_path = PathBuf::from(context);

        if context_path.is_absolute() {
            context_path
        } else {
            self.config_directory.join(context_path)
        }
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

    #[gpui::test]
    async fn check_for_existing_container_errors_when_multiple_match(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let (test_dependencies, devcontainer_manifest) =
            init_default_devcontainer_manifest(cx, r#"{"image": "image"}"#)
                .await
                .unwrap();
        test_dependencies
            .docker
            .set_duplicate_container_ids(vec!["abc123".to_string(), "def456".to_string()]);

        let result = devcontainer_manifest
            .check_for_existing_devcontainer()
            .await;

        let Err(DevContainerError::MultipleMatchingContainers(ids)) = result else {
            panic!("expected MultipleMatchingContainers, got {result:?}");
        };
        assert_eq!(ids, vec!["abc123".to_string(), "def456".to_string()]);
    }

    #[gpui::test]
    async fn trim_non_alphanumeric_chars_from_image_tag(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        env_logger::try_init().ok();
        let given_devcontainer_contents = r#"
            {
              "name": "abcde test",
              "image": "test_image:latest",
            }
            "#;

        let (_, devcontainer_manifest) =
            init_default_devcontainer_manifest(cx, given_devcontainer_contents)
                .await
                .unwrap();

        let image_tag = devcontainer_manifest.generate_features_image_tag("Dockerfile".to_string());

        assert!(
            image_tag.starts_with("abcde-"),
            "expected prefix 'abcde-', got: {image_tag}"
        );
        assert!(
            image_tag.ends_with("-features"),
            "expected suffix '-features', got: {image_tag}"
        );
    }

    #[test]
    fn test_aliases_dockerfile_with_pre_existing_aliases_for_build() {}

    #[test]
    fn test_aliases_dockerfile_with_no_aliases_for_build() {}

    #[test]
    fn test_aliases_dockerfile_with_build_target_specified() {}
}
