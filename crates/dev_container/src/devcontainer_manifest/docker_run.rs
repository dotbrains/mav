use super::*;

impl DevContainerManifest {
    pub(super) fn create_docker_build(&self) -> Result<Command, DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet proceed with docker build"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };

        let Some(features_build_info) = &self.features_build_info else {
            log::error!(
                "Cannot create docker build command; features build info has not been constructed"
            );
            return Err(DevContainerError::DevContainerParseFailed);
        };
        let mut command = Command::new(self.docker_client.docker_cli());

        command.args(["buildx", "build"]);

        // --load is short for --output=docker, loading the built image into the local docker images
        command.arg("--load");

        // BuildKit build context: provides the features content directory as a named context
        // that the Dockerfile.extended can COPY from via `--from=dev_containers_feature_content_source`
        command.args([
            "--build-context",
            &format!(
                "dev_containers_feature_content_source={}",
                features_build_info.features_content_dir.display()
            ),
        ]);

        // Build args matching the CLI reference implementation's `getFeaturesBuildOptions`
        if let Some(build_image) = &features_build_info.build_image {
            command.args([
                "--build-arg",
                &format!("_DEV_CONTAINERS_BASE_IMAGE={}", build_image),
            ]);
        } else {
            command.args([
                "--build-arg",
                "_DEV_CONTAINERS_BASE_IMAGE=dev_container_auto_added_stage_label",
            ]);
        }

        command.args([
            "--build-arg",
            &format!(
                "_DEV_CONTAINERS_IMAGE_USER={}",
                self.root_image
                    .as_ref()
                    .and_then(|docker_image| docker_image.config.image_user.as_ref())
                    .unwrap_or(&"root".to_string())
            ),
        ]);

        command.args([
            "--build-arg",
            "_DEV_CONTAINERS_FEATURE_CONTENT_SOURCE=dev_container_feature_content_temp",
        ]);

        if let Some(args) = dev_container.build.as_ref().and_then(|b| b.args.as_ref()) {
            for (key, value) in args {
                command.args(["--build-arg", &format!("{}={}", key, value)]);
            }
        }

        if let Some(options) = dev_container
            .build
            .as_ref()
            .and_then(|b| b.options.as_ref())
        {
            for option in options {
                command.arg(option);
            }
        }

        if let Some(cache_from_images) = dev_container
            .build
            .as_ref()
            .and_then(|b| b.cache_from.as_ref())
        {
            for cache_from_image in cache_from_images {
                command.args(["--cache-from", cache_from_image]);
            }
        }

        command.args(["--target", "dev_containers_target_stage"]);

        command.args([
            "-f",
            &features_build_info.dockerfile_path.display().to_string(),
        ]);

        command.args(["-t", &features_build_info.image_tag]);

        if let DevContainerBuildType::Dockerfile(build) = dev_container.build_type() {
            command.arg(self.calculate_context_dir(build).display().to_string());
        } else {
            // Use an empty folder as the build context to avoid pulling in unneeded files.
            // The actual feature content is supplied via the BuildKit build context above.
            command.arg(features_build_info.empty_context_dir.display().to_string());
        }

        Ok(command)
    }

    pub(super) async fn run_docker_compose(
        &self,
        resources: DockerComposeResources,
    ) -> Result<DockerInspect, DevContainerError> {
        let mut command = Command::new(self.docker_client.docker_cli());
        let project_name = self.project_name().await?;
        command.args(&["compose", "--project-name", &project_name]);
        for docker_compose_file in resources.files {
            command.args(&["-f", &docker_compose_file.display().to_string()]);
        }
        command.args(&["up", "-d"]);
        if let Some(run_services) = self.dev_container().run_services.as_ref() {
            command.args(run_services);
        }

        let output = self
            .command_runner
            .run_command(&mut command)
            .await
            .map_err(|e| {
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

        if let Some(docker_ps) = self.check_for_existing_container().await? {
            log::debug!("Found newly created dev container");
            return self.docker_client.inspect(&docker_ps.id).await;
        }

        log::error!("Could not find existing container after docker compose up");

        Err(DevContainerError::DevContainerParseFailed)
    }

    pub(super) async fn run_docker_image(
        &self,
        build_resources: DockerBuildResources,
    ) -> Result<DockerInspect, DevContainerError> {
        let mut docker_run_command = self.create_docker_run_command(build_resources)?;

        let output = self
            .command_runner
            .run_command(&mut docker_run_command)
            .await
            .map_err(|e| {
                log::error!("Error running docker run: {e}");
                DevContainerError::CommandFailed(
                    docker_run_command.get_program().display().to_string(),
                )
            })?;

        if !output.status.success() {
            let std_err = String::from_utf8_lossy(&output.stderr);
            log::error!("Non-success status from docker run. StdErr: {std_err}");
            return Err(DevContainerError::CommandFailed(
                docker_run_command.get_program().display().to_string(),
            ));
        }

        log::debug!("Checking for container that was started");
        let Some(docker_ps) = self.check_for_existing_container().await? else {
            log::error!("Could not locate container just created");
            return Err(DevContainerError::DevContainerParseFailed);
        };
        self.docker_client.inspect(&docker_ps.id).await
    }

    pub(super) fn local_workspace_folder(&self) -> String {
        self.local_project_directory.display().to_string()
    }
    pub(super) fn local_workspace_base_name(&self) -> Result<String, DevContainerError> {
        self.local_project_directory
            .file_name()
            .map(|f| f.display().to_string())
            .ok_or(DevContainerError::DevContainerParseFailed)
    }

    pub(super) fn remote_workspace_folder(&self) -> Result<PathBuf, DevContainerError> {
        self.dev_container()
            .workspace_folder
            .as_ref()
            .map(|folder| PathBuf::from(folder))
            .or(Some(
                // We explicitly use "/" here, instead of PathBuf::join
                // because we want remote targets to use unix-style filepaths,
                // even on a Windows host
                PathBuf::from(format!(
                    "{}/{}",
                    DEFAULT_REMOTE_PROJECT_DIR,
                    self.local_workspace_base_name()?
                )),
            ))
            .ok_or(DevContainerError::DevContainerParseFailed)
    }
    pub(super) fn remote_workspace_base_name(&self) -> Result<String, DevContainerError> {
        self.remote_workspace_folder().and_then(|f| {
            f.file_name()
                .map(|file_name| file_name.display().to_string())
                .ok_or(DevContainerError::DevContainerParseFailed)
        })
    }

    fn remote_workspace_mount(&self) -> Result<MountDefinition, DevContainerError> {
        if let Some(mount) = &self.dev_container().workspace_mount {
            return Ok(mount.clone());
        }
        let Some(project_directory_name) = self.local_project_directory.file_name() else {
            return Err(DevContainerError::DevContainerParseFailed);
        };

        Ok(MountDefinition {
            source: Some(self.local_workspace_folder()),
            // We explicitly use "/" here, instead of PathBuf::join
            // because we want the remote target to use unix-style filepaths,
            // even on a Windows host
            target: format!(
                "{}/{}",
                PathBuf::from(DEFAULT_REMOTE_PROJECT_DIR).display(),
                project_directory_name.display()
            ),
            mount_type: None,
        })
    }

    fn create_docker_run_command(
        &self,
        build_resources: DockerBuildResources,
    ) -> Result<Command, DevContainerError> {
        let remote_workspace_mount = self.remote_workspace_mount()?;

        let docker_cli = self.docker_client.docker_cli();
        let mut command = Command::new(&docker_cli);

        command.arg("run");

        if build_resources.privileged {
            command.arg("--privileged");
        }

        let run_args = match &self.dev_container().run_args {
            Some(run_args) => run_args,
            None => &Vec::new(),
        };

        for arg in run_args {
            command.arg(arg);
        }

        let run_if_missing = {
            |arg_name: &str, arg: &str, command: &mut Command| {
                if !run_args
                    .iter()
                    .any(|arg| arg.strip_prefix(arg_name).is_some())
                {
                    command.arg(arg);
                }
            }
        };

        if &docker_cli == "podman" {
            run_if_missing(
                "--security-opt",
                "--security-opt=label=disable",
                &mut command,
            );
            run_if_missing("--userns", "--userns=keep-id", &mut command);
        }

        run_if_missing("--sig-proxy", "--sig-proxy=false", &mut command);
        command.arg("-d");
        command.arg("--mount");
        command.arg(remote_workspace_mount.to_string());

        for mount in &build_resources.additional_mounts {
            command.arg("--mount");
            command.arg(mount.to_string());
        }

        for (key, val) in self.identifying_labels() {
            command.arg("-l");
            command.arg(format!("{}={}", key, val));
        }

        if let Some(metadata) = &build_resources.image.config.labels.metadata {
            let serialized_metadata = serde_json_lenient::to_string(metadata).map_err(|e| {
                log::error!("Problem serializing image metadata: {e}");
                DevContainerError::ContainerNotValid(build_resources.image.id.clone())
            })?;
            command.arg("-l");
            command.arg(format!(
                "{}={}",
                "devcontainer.metadata", serialized_metadata
            ));
        }

        if let Some(forward_ports) = &self.dev_container().forward_ports {
            for port in forward_ports {
                if let ForwardPort::Number(port_number) = port {
                    command.arg("-p");
                    command.arg(format!("{port_number}:{port_number}"));
                }
            }
        }
        for app_port in &self.dev_container().app_port {
            command.arg("-p");
            command.arg(app_port);
        }

        if let Some(entrypoint_script) = build_resources.entrypoint_script {
            command.arg("--entrypoint");
            command.arg("/bin/sh");
            command.arg(&build_resources.image.id);
            command.arg("-c");
            command.arg(entrypoint_script);
            command.arg("-");
        } else {
            command.arg(&build_resources.image.id);
        }

        Ok(command)
    }
}
