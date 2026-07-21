use super::*;

impl DevContainerManifest {
    pub(super) async fn project_name(&self) -> Result<String, DevContainerError> {
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

    pub(super) async fn expanded_dockerfile_content(&self) -> Result<String, DevContainerError> {
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

    pub(super) fn calculate_context_dir(&self, build: ContainerBuild) -> PathBuf {
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
