use super::*;

impl DevContainerManifest {
    pub(super) async fn dockerfile_location(&self) -> Option<PathBuf> {
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

    pub(super) fn generate_features_image_tag(&self, dockerfile_build_path: String) -> String {
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
    pub(super) async fn get_base_image_from_config(&self) -> Result<String, DevContainerError> {
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

    pub(super) fn generate_dockerfile_extended(
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
}
