use super::*;

impl DevContainerManifest {
    async fn copy_local_feature(
        &self,
        feature_ref: &str,
        destination: &Path,
    ) -> Result<(), DevContainerError> {
        let source_path = normalize_path(&self.config_directory.join(feature_ref));

        if !self.fs.is_dir(&source_path).await {
            log::error!(
                "Local feature directory '{}' not found at {:?}",
                feature_ref,
                source_path
            );
            return Err(DevContainerError::ResourceFetchFailed);
        }

        let items = fs::read_dir_items(&*self.fs, &source_path)
            .await
            .map_err(|e| {
                log::error!(
                    "Failed to read local feature directory {:?}: {e}",
                    source_path
                );
                DevContainerError::FilesystemError
            })?;

        for (item_path, is_dir) in &items {
            let relative = item_path.strip_prefix(&source_path).map_err(|e| {
                log::error!("Failed to compute relative path for {:?}: {e}", item_path);
                DevContainerError::FilesystemError
            })?;
            let dest_path = destination.join(relative);

            if *is_dir {
                self.fs.create_dir(&dest_path).await.map_err(|e| {
                    log::error!("Failed to create directory {:?}: {e}", dest_path);
                    DevContainerError::FilesystemError
                })?;
            } else {
                let content = self.fs.load_bytes(item_path).await.map_err(|e| {
                    log::error!("Failed to read file {:?}: {e}", item_path);
                    DevContainerError::FilesystemError
                })?;
                self.fs.write(&dest_path, &content).await.map_err(|e| {
                    log::error!("Failed to write file {:?}: {e}", dest_path);
                    DevContainerError::FilesystemError
                })?;
            }
        }

        Ok(())
    }

    pub(super) async fn download_feature_and_dockerfile_resources(
        &mut self,
    ) -> Result<(), DevContainerError> {
        let dev_container = match &self.config {
            ConfigStatus::Deserialized(_) => {
                log::error!(
                    "Dev container has not yet been parsed for variable expansion. Cannot yet download resources"
                );
                return Err(DevContainerError::DevContainerParseFailed);
            }
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        };
        let root_image_tag = self.get_base_image_from_config().await?;
        let root_image = self.docker_client.inspect(&root_image_tag).await?;

        let temp_base = std::env::temp_dir().join("devcontainer-mav");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let features_content_dir = temp_base.join(format!("container-features-{}", timestamp));
        let empty_context_dir = temp_base.join("empty-folder");

        self.fs
            .create_dir(&features_content_dir)
            .await
            .map_err(|e| {
                log::error!("Failed to create features content dir: {e}");
                DevContainerError::FilesystemError
            })?;

        self.fs.create_dir(&empty_context_dir).await.map_err(|e| {
            log::error!("Failed to create empty context dir: {e}");
            DevContainerError::FilesystemError
        })?;

        let dockerfile_path = features_content_dir.join("Dockerfile.extended");
        let image_tag =
            self.generate_features_image_tag(dockerfile_path.clone().display().to_string());

        let build_info = FeaturesBuildInfo {
            dockerfile_path,
            features_content_dir,
            empty_context_dir,
            build_image: dev_container.image.clone(),
            image_tag,
        };

        let features = match &dev_container.features {
            Some(features) => features,
            None => &HashMap::new(),
        };

        let container_user = get_container_user_from_config(&root_image, self)?;
        let remote_user = get_remote_user_from_config(&root_image, self)?;

        let builtin_env_content = format!(
            "_CONTAINER_USER={}\n_REMOTE_USER={}\n",
            container_user, remote_user
        );

        let builtin_env_path = build_info
            .features_content_dir
            .join("devcontainer-features.builtin.env");

        self.fs
            .write(&builtin_env_path, &builtin_env_content.as_bytes())
            .await
            .map_err(|e| {
                log::error!("Failed to write builtin env file: {e}");
                DevContainerError::FilesystemError
            })?;

        let ordered_features =
            resolve_feature_order(features, &dev_container.override_feature_install_order);

        for (index, (feature_ref, options)) in ordered_features.iter().enumerate() {
            if matches!(options, FeatureOptions::Bool(false)) {
                log::debug!(
                    "Feature '{}' is disabled (set to false), skipping",
                    feature_ref
                );
                continue;
            }

            let feature_id = extract_feature_id(feature_ref);
            let consecutive_id = format!("{}_{}", feature_id, index);
            let feature_dir = build_info.features_content_dir.join(&consecutive_id);

            self.fs.create_dir(&feature_dir).await.map_err(|e| {
                log::error!(
                    "Failed to create feature directory for {}: {e}",
                    feature_ref
                );
                DevContainerError::FilesystemError
            })?;

            if is_local_feature_ref(feature_ref) {
                self.copy_local_feature(feature_ref, &feature_dir).await?;
            } else {
                let oci_ref = parse_oci_feature_ref(feature_ref).ok_or_else(|| {
                    log::error!(
                        "Feature '{}' is not a supported OCI feature reference",
                        feature_ref
                    );
                    DevContainerError::DevContainerParseFailed
                })?;
                let TokenResponse { token } =
                    get_oci_token(&oci_ref.registry, &oci_ref.path, &self.http_client)
                        .await
                        .map_err(|e| {
                            log::error!(
                                "Failed to get OCI token for feature '{}': {e}",
                                feature_ref
                            );
                            DevContainerError::ResourceFetchFailed
                        })?;
                let manifest = get_oci_manifest(
                    &oci_ref.registry,
                    &oci_ref.path,
                    &token,
                    &self.http_client,
                    &oci_ref.version,
                    None,
                )
                .await
                .map_err(|e| {
                    log::error!(
                        "Failed to fetch OCI manifest for feature '{}': {e}",
                        feature_ref
                    );
                    DevContainerError::ResourceFetchFailed
                })?;
                let digest = &manifest
                    .layers
                    .first()
                    .ok_or_else(|| {
                        log::error!(
                            "OCI manifest for feature '{}' contains no layers",
                            feature_ref
                        );
                        DevContainerError::ResourceFetchFailed
                    })?
                    .digest;
                download_oci_tarball(
                    &token,
                    &oci_ref.registry,
                    &oci_ref.path,
                    digest,
                    "application/vnd.devcontainers.layer.v1+tar",
                    &feature_dir,
                    &self.http_client,
                    &self.fs,
                    None,
                )
                .await?;
            }

            let feature_json_path = &feature_dir.join("devcontainer-feature.json");
            if !self.fs.is_file(feature_json_path).await {
                let message = format!(
                    "No devcontainer-feature.json found in {:?}, no defaults to apply",
                    feature_json_path
                );
                log::error!("{}", &message);
                return Err(DevContainerError::ResourceFetchFailed);
            }

            let contents = self.fs.load(&feature_json_path).await.map_err(|e| {
                log::error!("error reading devcontainer-feature.json: {:?}", e);
                DevContainerError::FilesystemError
            })?;

            let contents_parsed = self.parse_nonremote_vars_for_content(&contents)?;

            let feature_json: DevContainerFeatureJson =
                serde_json_lenient::from_value(contents_parsed).map_err(|e| {
                    log::error!("Failed to parse devcontainer-feature.json: {e}");
                    DevContainerError::ResourceFetchFailed
                })?;

            let feature_manifest = FeatureManifest::new(consecutive_id, feature_dir, feature_json);

            log::debug!("Prepared feature content for '{}'", feature_ref);

            let env_content = feature_manifest
                .write_feature_env(&self.fs, options)
                .await?;

            let wrapper_content = generate_install_wrapper(feature_ref, feature_id, &env_content)?;

            self.fs
                .write(
                    &feature_manifest
                        .file_path()
                        .join("devcontainer-features-install.sh"),
                    &wrapper_content.as_bytes(),
                )
                .await
                .map_err(|e| {
                    log::error!("Failed to write install wrapper for {}: {e}", feature_ref);
                    DevContainerError::FilesystemError
                })?;

            self.features.push(feature_manifest);
        }

        // --- Phase 3: Generate extended Dockerfile from the inflated manifests ---

        let is_compose = match dev_container.build_type() {
            DevContainerBuildType::DockerCompose => true,
            _ => false,
        };
        let use_buildkit = self.docker_client.supports_compose_buildkit() || !is_compose;

        let dockerfile_base_content = if let Some(location) = &self.dockerfile_location().await {
            self.fs.load(location).await.log_err()
        } else {
            None
        };

        let build_target = if is_compose {
            find_primary_service(&self.docker_compose_manifest().await?, self)?
                .1
                .build
                .and_then(|b| b.target)
        } else {
            dev_container.build.as_ref().and_then(|b| b.target.clone())
        };

        let dockerfile_content = dockerfile_base_content
            .map(|content| {
                dockerfile_inject_alias(
                    &content,
                    "dev_container_auto_added_stage_label",
                    build_target,
                )
            })
            .unwrap_or_default();

        let dockerfile_content = self.generate_dockerfile_extended(
            &container_user,
            &remote_user,
            dockerfile_content,
            use_buildkit,
        );

        self.fs
            .write(&build_info.dockerfile_path, &dockerfile_content.as_bytes())
            .await
            .map_err(|e| {
                log::error!("Failed to write Dockerfile.extended: {e}");
                DevContainerError::FilesystemError
            })?;

        log::debug!(
            "Features build resources written to {:?}",
            build_info.features_content_dir
        );

        self.root_image = Some(root_image);
        self.features_build_info = Some(build_info);

        Ok(())
    }
}
