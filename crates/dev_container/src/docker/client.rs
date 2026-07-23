impl Docker {
    pub(crate) async fn new(docker_cli: &str, use_buildkit: Option<bool>) -> Self {
        let has_buildx = if docker_cli == "podman" {
            false
        } else if let Some(use_buildkit) = use_buildkit {
            // Honor the explicit `dev_container_use_buildkit` setting. Setting it
            // to `false` forces the classic Docker builder for Docker-compatible
            // engines that lack an integrated BuildKit (e.g. Apple Container via
            // a Docker-API bridge), where BuildKit builds cannot resolve
            // locally-built images. The classic builder builds the feature
            // content as an image and references it with an ordinary
            // multi-stage `FROM`.
            use_buildkit
        } else {
            let output = Command::new(docker_cli)
                .args(["buildx", "version"])
                .output()
                .await;
            output.map(|o| o.status.success()).unwrap_or(false)
        };
        if !has_buildx && docker_cli != "podman" {
            log::info!(
                "Using the classic Docker builder for dev container builds (BuildKit unavailable or disabled)"
            );
        }
        Self {
            docker_cli: docker_cli.to_string(),
            has_buildx,
        }
    }

    fn is_podman(&self) -> bool {
        self.docker_cli == "podman"
    }

    async fn pull_image(&self, image: &String) -> Result<(), DevContainerError> {
        let mut command = Command::new(&self.docker_cli);
        command.args(&["pull", "--", image]);

        let output = command.output().await.map_err(|e| {
            log::error!("Error pulling image: {e}");
            DevContainerError::ResourceFetchFailed
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Non-success result from docker pull: {stderr}");
            return Err(DevContainerError::ResourceFetchFailed);
        }
        Ok(())
    }

    fn create_docker_query_containers(&self, filters: Vec<String>) -> Command {
        let mut command = Command::new(&self.docker_cli);
        command.args(&["ps", "-a"]);

        for filter in filters {
            command.arg("--filter");
            command.arg(filter);
        }
        command.arg("--format={{ json . }}");
        command
    }

    fn create_docker_inspect(&self, id: &str) -> Command {
        let mut command = Command::new(&self.docker_cli);
        command.args(&["inspect", "--format={{json . }}", id]);
        command
    }

    fn create_docker_compose_config_command(&self, config_files: &Vec<PathBuf>) -> Command {
        let mut command = Command::new(&self.docker_cli);
        command.arg("compose");
        for file_path in config_files {
            command.args(&["-f", &file_path.display().to_string()]);
        }
        command.arg("config");
        command
    }
}

#[async_trait]
impl DockerClient for Docker {
    async fn inspect(&self, id: &String) -> Result<DockerInspect, DevContainerError> {
        // Try to pull the image, continue on failure; Image may be local only, id a reference to a running container
        self.pull_image(id).await.ok();

        let command = self.create_docker_inspect(id);

        let Some(docker_inspect): Option<DockerInspect> = evaluate_json_command(command).await?
        else {
            log::error!("Docker inspect produced no deserializable output");
            return Err(DevContainerError::CommandFailed(self.docker_cli.clone()));
        };
        Ok(docker_inspect)
    }

    async fn get_docker_compose_config(
        &self,
        config_files: &Vec<PathBuf>,
    ) -> Result<Option<DockerComposeConfig>, DevContainerError> {
        let command = self.create_docker_compose_config_command(config_files);
        evaluate_yaml_command(command).await
    }

    async fn docker_compose_build(
        &self,
        config_files: &Vec<PathBuf>,
        project_name: &str,
        services: Option<&Vec<String>>,
    ) -> Result<(), DevContainerError> {
        let mut command = Command::new(&self.docker_cli);
        if !self.is_podman() {
            if self.has_buildx {
                command.env("DOCKER_BUILDKIT", "1");
            } else {
                // Without a usable BuildKit, build through the classic builder so
                // multi-stage `FROM` of locally-built images (the feature content
                // image) resolves from the daemon's image store.
                command.env("DOCKER_BUILDKIT", "0");
                command.env("COMPOSE_DOCKER_CLI_BUILD", "0");
            }
        }
        command.args(&["compose", "--project-name", project_name]);
        for docker_compose_file in config_files {
            command.args(&["-f", &docker_compose_file.display().to_string()]);
        }
        command.arg("build");
        if let Some(services) = services {
            command.args(services);
        }

        let output = command.output().await.map_err(|e| {
            log::error!("Error running docker compose up: {e}");
            DevContainerError::CommandFailed(command.get_program().display().to_string())
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Non-success status from docker compose up: {}", stderr);
            return Err(DevContainerError::CommandFailed(
                command.get_program().display().to_string(),
            ));
        }

        Ok(())
    }
    async fn run_docker_exec(
        &self,
        container_id: &str,
        remote_folder: &str,
        user: &str,
        env: &HashMap<String, String>,
        inner_command: Command,
    ) -> Result<(), DevContainerError> {
        let mut command = Command::new(&self.docker_cli);

        command.args(&["exec", "-w", remote_folder, "-u", user]);

        for (k, v) in env.iter() {
            command.arg("-e");
            let env_declaration = format!("{}={}", k, v);
            command.arg(&env_declaration);
        }

        command.arg(container_id);

        command.arg("sh");

        let mut inner_program_script: Vec<String> =
            vec![inner_command.get_program().display().to_string()];
        let mut args: Vec<String> = inner_command
            .get_args()
            .map(|arg| arg.display().to_string())
            .collect();
        inner_program_script.append(&mut args);
        command.args(&["-c", &inner_program_script.join(" ")]);

        let output = command.output().await.map_err(|e| {
            log::error!("Error running command {e} in container exec");
            DevContainerError::ContainerNotValid(container_id.to_string())
        })?;
        if !output.status.success() {
            let std_err = String::from_utf8_lossy(&output.stderr);
            log::error!("Command produced a non-successful output. StdErr: {std_err}");
        }
        let std_out = String::from_utf8_lossy(&output.stdout);
        log::debug!("Command output:\n {std_out}");

        Ok(())
    }
    async fn start_container(&self, id: &str) -> Result<(), DevContainerError> {
        let mut command = Command::new(&self.docker_cli);

        command.args(&["start", id]);

        let output = command.output().await.map_err(|e| {
            log::error!("Error running docker start: {e}");
            DevContainerError::CommandFailed(command.get_program().display().to_string())
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Non-success status from docker start: {stderr}");
            return Err(DevContainerError::CommandFailed(
                command.get_program().display().to_string(),
            ));
        }

        Ok(())
    }

    async fn find_process_by_filters(
        &self,
        filters: Vec<String>,
    ) -> Result<Option<DockerPs>, DevContainerError> {
        let mut command = self.create_docker_query_containers(filters);
        let output = command.output().await.map_err(|e| {
            log::error!("Error running command {:?}: {e}", command);
            DevContainerError::CommandFailed(command.get_program().display().to_string())
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("Non-success status from docker ps: {stderr}");
            return Err(DevContainerError::CommandFailed(
                command.get_program().display().to_string(),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        parse_find_process_output(&raw).map_err(|e| {
            // Preserve the dedicated multi-match error; log and re-wrap other parse failures.
            if let DevContainerError::MultipleMatchingContainers(_) = &e {
                e
            } else {
                log::error!("Error parsing docker ps output: {e}");
                DevContainerError::CommandFailed(command.get_program().display().to_string())
            }
        })
    }

    fn docker_cli(&self) -> String {
        self.docker_cli.clone()
    }

    fn supports_compose_buildkit(&self) -> bool {
        self.has_buildx
    }
}

/// Parses output of `docker ps -a --format={{ json . }}`. When a single
/// container matches the label filters, docker emits one JSON object; when
/// multiple match, it emits newline-delimited JSON (one object per line).
///
/// Returns `Ok(None)` for no matches, `Ok(Some(_))` for exactly one match,
/// and `DevContainerError::MultipleMatchingContainers` for ≥2 matches — the
/// spec expects identifying labels to be unique per project, so the caller
/// can't silently pick one.
pub(super) fn parse_find_process_output(raw: &str) -> Result<Option<DockerPs>, DevContainerError> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let containers: Vec<DockerPs> = serde_json_lenient::Deserializer::from_str(raw)
        .into_iter::<DockerPs>()
        .collect::<Result<_, _>>()
        .map_err(|e| {
            DevContainerError::CommandFailed(format!("failed to parse docker ps output: {e}"))
        })?;
    match containers.len() {
        0 => Ok(None),
        1 => Ok(containers.into_iter().next()),
        _ => Err(DevContainerError::MultipleMatchingContainers(
            containers.into_iter().map(|c| c.id).collect(),
        )),
    }
}

#[async_trait]
pub(crate) trait DockerClient {
    async fn inspect(&self, id: &String) -> Result<DockerInspect, DevContainerError>;
    async fn get_docker_compose_config(
        &self,
        config_files: &Vec<PathBuf>,
    ) -> Result<Option<DockerComposeConfig>, DevContainerError>;
    async fn docker_compose_build(
        &self,
        config_files: &Vec<PathBuf>,
        project_name: &str,
        services: Option<&Vec<String>>,
    ) -> Result<(), DevContainerError>;
    async fn run_docker_exec(
        &self,
        container_id: &str,
        remote_folder: &str,
        user: &str,
        env: &HashMap<String, String>,
        inner_command: Command,
    ) -> Result<(), DevContainerError>;
    async fn start_container(&self, id: &str) -> Result<(), DevContainerError>;
    async fn find_process_by_filters(
        &self,
        filters: Vec<String>,
    ) -> Result<Option<DockerPs>, DevContainerError>;
    fn supports_compose_buildkit(&self) -> bool;
    /// This operates as an escape hatch for more custom uses of the docker API.
    /// See DevContainerManifest::create_docker_build as an example
    fn docker_cli(&self) -> String;
}
use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use util::command::Command;

use crate::{
    command_json::{evaluate_json_command, evaluate_yaml_command},
    devcontainer_api::DevContainerError,
};

use super::{Docker, DockerComposeConfig, DockerInspect, DockerPs};
