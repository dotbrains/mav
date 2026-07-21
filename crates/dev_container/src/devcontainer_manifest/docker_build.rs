use super::*;

impl DevContainerManifest {
    pub(super) async fn build_docker_image(&self) -> Result<DockerInspect, DevContainerError> {
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
    pub(super) async fn update_remote_user_uid(
        &self,
        image: DockerInspect,
        _base_image: &str,
    ) -> Result<DockerInspect, DevContainerError> {
        Ok(image)
    }
    #[cfg(not(target_os = "windows"))]
    pub(super) async fn update_remote_user_uid(
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
    pub(super) fn generate_update_uid_dockerfile(&self) -> String {
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

    pub(super) async fn build_feature_content_image(&self) -> Result<(), DevContainerError> {
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
}
