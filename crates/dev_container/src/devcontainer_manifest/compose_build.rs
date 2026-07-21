use super::*;

impl DevContainerManifest {
    pub(super) async fn docker_compose_manifest(
        &self,
    ) -> Result<DockerComposeResources, DevContainerError> {
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
    pub(super) async fn build_and_extend_compose_files(
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

    pub(super) async fn write_runtime_override_file(
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

    pub(super) fn build_runtime_override(
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
}
