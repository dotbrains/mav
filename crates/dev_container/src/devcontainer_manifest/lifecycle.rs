use super::*;

impl DevContainerManifest {
    pub(super) fn extension_ids(&self) -> Vec<String> {
        self.dev_container()
            .customizations
            .as_ref()
            .map(|c| c.mav.extensions.clone())
            .unwrap_or_default()
    }

    pub(super) async fn build_and_run(&mut self) -> Result<DevContainerUp, DevContainerError> {
        self.dev_container().validate_devcontainer_contents()?;

        self.run_initialize_commands().await?;

        self.download_feature_and_dockerfile_resources().await?;

        let build_resources = self.build_resources().await?;

        let devcontainer_up = self.run_dev_container(build_resources).await?;

        self.run_remote_scripts(&devcontainer_up, true).await?;

        Ok(devcontainer_up)
    }

    pub(super) async fn run_remote_scripts(
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

    pub(super) async fn run_initialize_commands(&self) -> Result<(), DevContainerError> {
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

    pub(super) async fn check_for_existing_devcontainer(
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

    pub(super) async fn check_for_existing_container(
        &self,
    ) -> Result<Option<DockerPs>, DevContainerError> {
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
