use super::*;

impl SshRemoteConnection {
    pub(super) async fn ensure_server_binary(
        &self,
        delegate: &Arc<dyn RemoteClientDelegate>,
        release_channel: ReleaseChannel,
        version: Version,
        cx: &mut AsyncApp,
    ) -> Result<Arc<RelPath>> {
        let version_str = match release_channel {
            ReleaseChannel::Dev => "build".to_string(),
            _ => version.to_string(),
        };
        let binary_name = format!(
            "mav-remote-server-{}-{}{}",
            release_channel.dev_name(),
            version_str,
            if self.ssh_platform.os.is_windows() {
                ".exe"
            } else {
                ""
            }
        );
        let dst_path =
            paths::remote_server_dir_relative().join(RelPath::unix(&binary_name).unwrap());

        let binary_exists_on_server = self
            .socket
            .run_command(
                self.ssh_shell_kind,
                &dst_path.display(self.path_style()),
                &["version"],
                true,
            )
            .await
            .is_ok();

        #[cfg(any(debug_assertions, feature = "build-remote-server-binary"))]
        if let Some(remote_server_path) = crate::transport::build_remote_server_from_source(
            &self.ssh_platform,
            delegate.as_ref(),
            binary_exists_on_server,
            cx,
        )
        .await?
        {
            let tmp_path = paths::remote_server_dir_relative().join(
                RelPath::unix(&format!(
                    "download-{}-{}",
                    std::process::id(),
                    remote_server_path.file_name().unwrap().to_string_lossy()
                ))
                .unwrap(),
            );
            self.upload_local_server_binary(&remote_server_path, &tmp_path, delegate, cx)
                .await?;
            self.extract_server_binary(&dst_path, &tmp_path, delegate, cx)
                .await?;
            return Ok(dst_path);
        }

        if binary_exists_on_server {
            return Ok(dst_path);
        }

        let wanted_version = cx.update(|cx| match release_channel {
            ReleaseChannel::Nightly => Ok(None),
            ReleaseChannel::Dev => {
                anyhow::bail!(
                    "MAV_BUILD_REMOTE_SERVER is not set and no remote server exists at ({:?})",
                    dst_path
                )
            }
            _ => Ok(Some(AppVersion::global(cx))),
        })?;

        let tmp_path_compressed = remote_server_dir_relative().join(
            RelPath::unix(&format!(
                "{}-download-{}.{}",
                binary_name,
                std::process::id(),
                if self.ssh_platform.os.is_windows() {
                    "zip"
                } else {
                    "gz"
                }
            ))
            .unwrap(),
        );
        if !self.socket.connection_options.upload_binary_over_ssh
            && let Some(url) = delegate
                .get_download_url(
                    self.ssh_platform,
                    release_channel,
                    wanted_version.clone(),
                    cx,
                )
                .await?
        {
            match self
                .download_binary_on_server(&url, &tmp_path_compressed, delegate, cx)
                .await
            {
                Ok(_) => {
                    self.extract_server_binary(&dst_path, &tmp_path_compressed, delegate, cx)
                        .await
                        .context("extracting server binary")?;
                    return Ok(dst_path);
                }
                Err(e) => {
                    log::error!(
                        "Failed to download binary on server, attempting to download locally and then upload it the server: {e:#}",
                    )
                }
            }
        }

        let src_path = delegate
            .download_server_binary_locally(
                self.ssh_platform,
                release_channel,
                wanted_version.clone(),
                cx,
            )
            .await
            .context("downloading server binary locally")?;
        self.upload_local_server_binary(&src_path, &tmp_path_compressed, delegate, cx)
            .await
            .context("uploading server binary")?;
        self.extract_server_binary(&dst_path, &tmp_path_compressed, delegate, cx)
            .await
            .context("extracting server binary")?;
        Ok(dst_path)
    }

    async fn download_binary_on_server(
        &self,
        url: &str,
        tmp_path: &RelPath,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        if let Some(parent) = tmp_path.parent() {
            let res = self
                .socket
                .run_command(
                    self.ssh_shell_kind,
                    "mkdir",
                    &["-p", parent.display(self.path_style()).as_ref()],
                    true,
                )
                .await;
            if !self.ssh_platform.os.is_windows() {
                // mkdir fails on windows if the path already exists ...
                res?;
            }
        }

        delegate.set_status(Some("Downloading remote development server on host"), cx);

        let connection_timeout = self
            .socket
            .connection_options
            .connection_timeout
            .unwrap_or(10)
            .to_string();

        match self
            .socket
            .run_command(
                self.ssh_shell_kind,
                "curl",
                &[
                    "-f",
                    "-L",
                    "--connect-timeout",
                    &connection_timeout,
                    url,
                    "-o",
                    &tmp_path.display(self.path_style()),
                ],
                true,
            )
            .await
        {
            Ok(_) => {}
            Err(e) => {
                if self
                    .socket
                    .run_command(self.ssh_shell_kind, "which", &["curl"], true)
                    .await
                    .is_ok()
                {
                    return Err(e);
                }

                log::info!("curl is not available, trying wget");
                match self
                    .socket
                    .run_command(
                        self.ssh_shell_kind,
                        "wget",
                        &[
                            "--connect-timeout",
                            &connection_timeout,
                            "--tries",
                            "1",
                            url,
                            "-O",
                            &tmp_path.display(self.path_style()),
                        ],
                        true,
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        if self
                            .socket
                            .run_command(self.ssh_shell_kind, "which", &["wget"], true)
                            .await
                            .is_ok()
                        {
                            return Err(e);
                        } else {
                            anyhow::bail!("Neither curl nor wget is available");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn upload_local_server_binary(
        &self,
        src_path: &Path,
        tmp_path: &RelPath,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        if let Some(parent) = tmp_path.parent() {
            let res = self
                .socket
                .run_command(
                    self.ssh_shell_kind,
                    "mkdir",
                    &["-p", parent.display(self.path_style()).as_ref()],
                    true,
                )
                .await;
            if !self.ssh_platform.os.is_windows() {
                // mkdir fails on windows if the path already exists ...
                res?;
            }
        }

        let src_stat = fs::metadata(&src_path)
            .await
            .with_context(|| format!("failed to get metadata for {:?}", src_path))?;
        let size = src_stat.len();

        let t0 = Instant::now();
        delegate.set_status(Some("Uploading remote development server"), cx);
        log::info!(
            "uploading remote development server to {:?} ({}kb)",
            tmp_path,
            size / 1024
        );
        self.upload_file(src_path, tmp_path)
            .await
            .context("failed to upload server binary")?;
        log::info!("uploaded remote development server in {:?}", t0.elapsed());
        Ok(())
    }

    async fn extract_server_binary(
        &self,
        dst_path: &RelPath,
        tmp_path: &RelPath,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        delegate.set_status(Some("Extracting remote development server"), cx);

        if self.ssh_platform.os.is_windows() {
            self.extract_server_binary_windows(dst_path, tmp_path).await
        } else {
            self.extract_server_binary_posix(dst_path, tmp_path).await
        }
    }

    async fn extract_server_binary_posix(
        &self,
        dst_path: &RelPath,
        tmp_path: &RelPath,
    ) -> Result<()> {
        let shell_kind = ShellKind::Posix;
        let server_mode = 0o755;
        let orig_tmp_path = tmp_path.display(self.path_style());
        let server_mode = format!("{:o}", server_mode);
        let server_mode = shell_kind
            .try_quote(&server_mode)
            .context("shell quoting")?;
        let dst_path = dst_path.display(self.path_style());
        let dst_path = shell_kind.try_quote(&dst_path).context("shell quoting")?;
        let script = if let Some(tmp_path) = orig_tmp_path.strip_suffix(".gz") {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            let tmp_path = shell_kind.try_quote(&tmp_path).context("shell quoting")?;
            format!(
                "gunzip -f {orig_tmp_path} && chmod {server_mode} {tmp_path} && mv {tmp_path} {dst_path}",
            )
        } else {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            format!("chmod {server_mode} {orig_tmp_path} && mv {orig_tmp_path} {dst_path}",)
        };
        let args = shell_kind.args_for_shell(false, script.to_string());
        self.socket
            .run_command(self.ssh_shell_kind, "sh", &args, true)
            .await?;
        Ok(())
    }

    async fn extract_server_binary_windows(
        &self,
        dst_path: &RelPath,
        tmp_path: &RelPath,
    ) -> Result<()> {
        let shell_kind = ShellKind::Pwsh;
        let orig_tmp_path = tmp_path.display(self.path_style());
        let dst_path = dst_path.display(self.path_style());
        let dst_path = shell_kind.try_quote(&dst_path).context("shell quoting")?;

        let script = if let Some(tmp_path) = orig_tmp_path.strip_suffix(".zip") {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            let tmp_path = shell_kind.try_quote(tmp_path).context("shell quoting")?;
            let tmp_exe_path = format!("{tmp_path}\\remote_server.exe");
            let tmp_exe_path = shell_kind
                .try_quote(&tmp_exe_path)
                .context("shell quoting")?;
            format!(
                "Expand-Archive -Force -Path {orig_tmp_path} -DestinationPath {tmp_path} -ErrorAction Stop; Move-Item -Force {tmp_exe_path} {dst_path}; Remove-Item -Force {tmp_path} -Recurse; Remove-Item -Force {orig_tmp_path}",
            )
        } else {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            format!("Move-Item -Force {orig_tmp_path} {dst_path}")
        };

        let args = shell_kind.args_for_shell(false, script);
        self.socket
            .run_command(self.ssh_shell_kind, "powershell", &args, true)
            .await?;
        Ok(())
    }

    pub(super) fn build_scp_command(
        &self,
        src_path: &Path,
        dest_path_str: &str,
        args: Option<&[&str]>,
    ) -> util::command::Command {
        let mut command = util::command::new_command("scp");
        self.socket.ssh_options(&mut command, false).args(
            self.socket
                .connection_options
                .port
                .map(|port| vec!["-P".to_string(), port.to_string()])
                .unwrap_or_default(),
        );
        if let Some(args) = args {
            command.args(args);
        }
        command.arg(src_path).arg(format!(
            "{}:{}",
            self.socket.connection_options.scp_destination(),
            dest_path_str
        ));
        command
    }

    pub(super) fn build_sftp_command(&self) -> util::command::Command {
        let mut command = util::command::new_command("sftp");
        self.socket.ssh_options(&mut command, false).args(
            self.socket
                .connection_options
                .port
                .map(|port| vec!["-P".to_string(), port.to_string()])
                .unwrap_or_default(),
        );
        command.arg("-b").arg("-");
        command.arg(self.socket.connection_options.scp_destination());
        command.stdin(Stdio::piped());
        command
    }

    async fn upload_file(&self, src_path: &Path, dest_path: &RelPath) -> Result<()> {
        log::debug!("uploading file {:?} to {:?}", src_path, dest_path);

        let src_path_display = src_path.display().to_string();
        let dest_path_str = dest_path.display(self.path_style());

        // We will try SFTP first, and if that fails, we will fall back to SCP.
        // If SCP fails also, we give up and return an error.
        // The reason we allow a fallback from SFTP to SCP is that if the user has to specify a password,
        // depending on the implementation of SSH stack, SFTP may disable interactive password prompts in batch mode.
        // This is for example the case on Windows as evidenced by this implementation snippet:
        // https://github.com/PowerShell/openssh-portable/blob/b8c08ef9da9450a94a9c5ef717d96a7bd83f3332/sshconnect2.c#L417
        if Self::is_sftp_available().await {
            log::debug!("using SFTP for file upload");
            let mut command = self.build_sftp_command();
            let sftp_batch = format!("put {src_path_display} {dest_path_str}\n");

            let mut child = command.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use futures::AsyncWriteExt;
                stdin.write_all(sftp_batch.as_bytes()).await?;
                stdin.flush().await?;
            }

            let output = child.output().await?;
            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!(
                "failed to upload file via SFTP {src_path_display} -> {dest_path_str}: {stderr}"
            );
        }

        log::debug!("using SCP for file upload");
        let mut command = self.build_scp_command(src_path, &dest_path_str, None);
        let output = command.output().await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!(
            "failed to upload file via SCP {src_path_display} -> {dest_path_str}: {stderr}",
        );
        anyhow::bail!(
            "failed to upload file via STFP/SCP {} -> {}: {}",
            src_path_display,
            dest_path_str,
            stderr,
        );
    }

    pub(super) async fn is_sftp_available() -> bool {
        which::which("sftp").is_ok()
    }
}
