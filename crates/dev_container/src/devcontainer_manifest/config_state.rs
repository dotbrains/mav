use super::*;

impl DevContainerManifest {
    pub(super) async fn new(
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

    pub(super) fn devcontainer_id(&self) -> String {
        let mut labels = self.identifying_labels();
        labels.sort_by_key(|(key, _)| *key);

        let mut hasher = DefaultHasher::new();
        for (key, value) in &labels {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }

    pub(super) fn identifying_labels(&self) -> Vec<(&str, String)> {
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

    pub(super) fn parse_nonremote_vars_for_content(
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

    pub(super) fn parse_nonremote_vars(&mut self) -> Result<(), DevContainerError> {
        let replaced_content = self.parse_nonremote_vars_for_content(&self.raw_config)?;
        let parsed_config = deserialize_devcontainer_json_from_value(replaced_content)?;

        self.config = ConfigStatus::VariableParsed(parsed_config);

        Ok(())
    }

    pub(super) fn runtime_remote_env(
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

    pub(super) fn replace_environment_variables(
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

    pub(super) fn config_file(&self) -> PathBuf {
        self.config_directory.join(&self.file_name)
    }

    pub(super) fn dev_container(&self) -> &DevContainer {
        match &self.config {
            ConfigStatus::Deserialized(dev_container) => dev_container,
            ConfigStatus::VariableParsed(dev_container) => dev_container,
        }
    }
}
