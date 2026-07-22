use super::*;

pub(super) struct LocalRegistryArchiveAgent {
    pub(super) fs: Arc<dyn Fs>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) node_runtime: NodeRuntime,
    pub(super) project_environment: Entity<ProjectEnvironment>,
    pub(super) registry_id: Arc<str>,
    pub(super) version: SharedString,
    pub(super) targets: HashMap<String, RegistryTargetConfig>,
    pub(super) env: HashMap<String, String>,
    pub(super) new_version_available_tx: Option<watch::Sender<Option<String>>>,
    pub(super) loading_status_tx: Option<watch::Sender<Option<String>>>,
}

impl ExternalAgentServer for LocalRegistryArchiveAgent {
    fn version(&self) -> Option<&SharedString> {
        Some(&self.version)
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.new_version_available_tx.take()
    }

    fn set_new_version_available_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.new_version_available_tx = Some(tx);
    }

    fn take_loading_status_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.loading_status_tx.take()
    }

    fn set_loading_status_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.loading_status_tx = Some(tx);
    }

    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let fs = self.fs.clone();
        let http_client = self.http_client.clone();
        let node_runtime = self.node_runtime.clone();
        let project_environment = self.project_environment.downgrade();
        let registry_id = self.registry_id.clone();
        let targets = self.targets.clone();
        let settings_env = self.env.clone();
        let version = self.version.clone();
        let loading_status_tx = self.loading_status_tx.take();

        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();

            let dir = paths::external_agents_dir()
                .join("registry")
                .join(sanitize_path_component(&registry_id));
            fs.create_dir(&dir).await?;

            let os = if cfg!(target_os = "macos") {
                "darwin"
            } else if cfg!(target_os = "linux") {
                "linux"
            } else if cfg!(target_os = "windows") {
                "windows"
            } else {
                anyhow::bail!("unsupported OS");
            };

            let arch = if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else if cfg!(target_arch = "x86_64") {
                "x86_64"
            } else {
                anyhow::bail!("unsupported architecture");
            };

            let platform_key = format!("{}-{}", os, arch);
            let target_config = targets.get(&platform_key).with_context(|| {
                format!(
                    "no target specified for platform '{}'. Available platforms: {}",
                    platform_key,
                    targets
                        .keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;

            env.extend(target_config.env.clone());
            env.extend(extra_env);
            env.extend(settings_env);

            let archive_url = &target_config.archive;
            let version_dir =
                versioned_archive_cache_dir(&dir, Some(version.as_ref()), archive_url);

            if !fs.is_dir(&version_dir).await {
                let mut loading_status_tx = loading_status_tx;
                if let Some(tx) = loading_status_tx.as_mut() {
                    tx.send(Some(format!("Installing {}…", version.as_ref())))
                        .ok();
                }

                let sha256 = if let Some(provided_sha) = &target_config.sha256 {
                    Some(provided_sha.clone())
                } else if let Some(github_archive) = github_release_archive_from_url(archive_url) {
                    if let Ok(release) = ::http_client::github::get_release_by_tag_name(
                        &github_archive.repo_name_with_owner,
                        &github_archive.tag,
                        http_client.clone(),
                    )
                    .await
                    {
                        if let Some(asset) = release
                            .assets
                            .iter()
                            .find(|a| a.name == github_archive.asset_name)
                        {
                            asset.digest.as_ref().and_then(|d| {
                                d.strip_prefix("sha256:")
                                    .map(|s| s.to_string())
                                    .or_else(|| Some(d.clone()))
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                match registry_archive_kind_for_url(archive_url)? {
                    RegistryArchiveKind::Archive(asset_kind) => {
                        ::http_client::github_download::download_server_binary(
                            &*http_client,
                            archive_url,
                            sha256.as_deref(),
                            &version_dir,
                            asset_kind,
                        )
                        .await?;
                    }
                    RegistryArchiveKind::RawBinary { file_name } => {
                        ::http_client::github_download::download_server_raw_binary(
                            &*http_client,
                            archive_url,
                            sha256.as_deref(),
                            &version_dir,
                            &file_name,
                        )
                        .await?;
                    }
                }
            }

            let cmd = &target_config.cmd;

            let cmd_path = if cmd == "node" {
                node_runtime.binary_path().await?
            } else {
                if cmd.contains("..") {
                    anyhow::bail!("command path cannot contain '..': {}", cmd);
                }

                if cmd.starts_with("./") || cmd.starts_with(".\\") {
                    let cmd_path = version_dir.join(&cmd[2..]);
                    anyhow::ensure!(
                        fs.is_file(&cmd_path).await,
                        "Missing command {} after extraction",
                        cmd_path.to_string_lossy()
                    );
                    cmd_path
                } else {
                    anyhow::bail!("command must be relative (start with './'): {}", cmd);
                }
            };

            cx.background_spawn({
                let fs = fs.clone();
                let dir = dir.clone();
                let version_dir = version_dir.clone();
                async move {
                    remove_stale_versioned_archive_cache_dirs(fs, &dir, &version_dir)
                        .await
                        .log_err();
                }
            })
            .detach();

            let mut args = target_config.args.clone();
            args.extend(extra_args);

            let command = AgentServerCommand {
                path: cmd_path,
                args,
                env: Some(env),
            };

            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub(super) struct LocalRegistryNpxAgent {
    pub(super) fs: Arc<dyn Fs>,
    pub(super) node_runtime: NodeRuntime,
    pub(super) project_environment: Entity<ProjectEnvironment>,
    pub(super) registry_id: Arc<str>,
    pub(super) version: SharedString,
    pub(super) package: SharedString,
    pub(super) args: Vec<String>,
    pub(super) distribution_env: HashMap<String, String>,
    pub(super) settings_env: HashMap<String, String>,
    pub(super) new_version_available_tx: Option<watch::Sender<Option<String>>>,
}

impl ExternalAgentServer for LocalRegistryNpxAgent {
    fn version(&self) -> Option<&SharedString> {
        Some(&self.version)
    }

    fn take_new_version_available_tx(&mut self) -> Option<watch::Sender<Option<String>>> {
        self.new_version_available_tx.take()
    }

    fn set_new_version_available_tx(&mut self, tx: watch::Sender<Option<String>>) {
        self.new_version_available_tx = Some(tx);
    }

    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let fs = self.fs.clone();
        let node_runtime = self.node_runtime.clone();
        let project_environment = self.project_environment.downgrade();
        let registry_id = self.registry_id.clone();
        let package = bounded_npm_package_spec(&self.package);
        let args = self.args.clone();
        let distribution_env = self.distribution_env.clone();
        let settings_env = self.settings_env.clone();

        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();

            let prefix_dir = paths::external_agents_dir()
                .join("registry")
                .join("npx")
                .join(sanitize_path_component(&registry_id));
            fs.create_dir(&prefix_dir).await?;

            let mut exec_args = vec!["--yes".to_string(), "--".to_string(), package];
            exec_args.extend(args);

            let npm_command = node_runtime
                .npm_command(
                    Some(&prefix_dir),
                    "exec",
                    &exec_args.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                )
                .await?;

            env.extend(npm_command.env);
            env.extend(distribution_env);
            env.extend(extra_env);
            env.extend(settings_env);

            let mut args = npm_command.args;
            args.extend(extra_args);

            let command = AgentServerCommand {
                path: npm_command.path,
                args,
                env: Some(env),
            };

            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// People are using min-release-age more frequently. Which means a fresh registry will likely have
/// new package versions than the user can install.
/// We set the version to now be a ceiling and not an exact pin instead. This allows npm to resolve
/// the latest version it can find that satisfies the constraint. npm seems to check regularly enough
/// that new versions are available. This does have a few downsides:
/// - The user might have an older cached version of the package that satisfies the constraint, until
///   npm checks for updates again.
/// - The registry args/env may not be valid for the resolved version.
///
/// This is a best-effort attempt to install a version that works without overriding the user's
/// security settings, as the args don't change often. The registry will need to support this better
/// at some point, but until then, this is a best-effort workaround that hopefully solves the issue
/// for most users.
///
/// We use npm's hyphen-range syntax (`0.0.0 - <version>`, equivalent to `<=<version>`) instead of
/// the more compact `<=<version>` form because on Windows, `npm` is `npm.cmd` (a batch file run by
/// cmd.exe), and the quotes our shell builder emits are PowerShell string-literal syntax that PS
/// strips during parsing. PS only re-adds CRT-style transport quotes around native command args
/// containing whitespace, so `package@<=0.25.3` reaches cmd.exe bare and the unquoted `<` is
/// interpreted as input redirection. See mav-industries/mav#55921.
pub(super) fn bounded_npm_package_spec(package_spec: &str) -> String {
    let Some((package_name, version)) = package_spec.rsplit_once('@') else {
        return package_spec.to_string();
    };
    if package_name.is_empty() || Version::parse(version).is_err() {
        return package_spec.to_string();
    }

    format!("{package_name}@0.0.0 - {version}")
}

pub(super) struct LocalCustomAgent {
    pub(super) project_environment: Entity<ProjectEnvironment>,
    pub(super) command: AgentServerCommand,
}

impl ExternalAgentServer for LocalCustomAgent {
    fn get_command(
        &mut self,
        extra_args: Vec<String>,
        extra_env: HashMap<String, String>,
        cx: &mut AsyncApp,
    ) -> Task<Result<AgentServerCommand>> {
        let mut command = self.command.clone();
        let project_environment = self.project_environment.downgrade();
        cx.spawn(async move |cx| {
            let mut env = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.default_environment(cx)
                })?
                .await
                .unwrap_or_default();
            env.extend(command.env.unwrap_or_default());
            env.extend(extra_env);
            command.env = Some(env);
            command.args.extend(extra_args);
            Ok(command)
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
