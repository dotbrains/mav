use crate::{
    RemoteArch, RemoteClientDelegate, RemoteOs, RemotePlatform,
    remote_client::{CommandTemplate, Interactive, RemoteConnection, RemoteConnectionOptions},
    transport::{parse_platform, parse_shell},
};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use collections::HashMap;
use futures::channel::mpsc::{Sender, UnboundedReceiver, UnboundedSender};
use gpui::{App, AppContext as _, AsyncApp, Task};
use release_channel::{AppVersion, ReleaseChannel};
use rpc::proto::Envelope;
use semver::Version;
use smol::fs;
use std::{
    ffi::OsStr,
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use util::{
    command::Stdio,
    paths::{PathStyle, RemotePathBuf},
    rel_path::RelPath,
    shell::{Shell, ShellKind},
    shell_builder::ShellBuilder,
};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct WslConnectionOptions {
    pub distro_name: String,
    pub user: Option<String>,
}

impl From<settings::WslConnection> for WslConnectionOptions {
    fn from(val: settings::WslConnection) -> Self {
        WslConnectionOptions {
            distro_name: val.distro_name,
            user: val.user,
        }
    }
}

#[derive(Debug)]
pub(crate) struct WslRemoteConnection {
    remote_binary_path: Option<Arc<RelPath>>,
    platform: RemotePlatform,
    os_version: Option<String>,
    shell: String,
    shell_kind: ShellKind,
    default_system_shell: String,
    has_wsl_interop: bool,
    connection_options: WslConnectionOptions,
}

impl WslRemoteConnection {
    pub(crate) async fn new(
        connection_options: WslConnectionOptions,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        log::info!(
            "Connecting to WSL distro {} with user {:?}",
            connection_options.distro_name,
            connection_options.user
        );
        let (release_channel, version) =
            cx.update(|cx| (ReleaseChannel::global(cx), AppVersion::global(cx)));

        let mut this = Self {
            connection_options,
            remote_binary_path: None,
            platform: RemotePlatform {
                os: RemoteOs::Linux,
                arch: RemoteArch::X86_64,
            },
            os_version: None,
            shell: String::new(),
            shell_kind: ShellKind::Posix,
            default_system_shell: String::from("/bin/sh"),
            has_wsl_interop: false,
        };
        delegate.set_status(Some("Detecting WSL environment"), cx);
        this.shell = this
            .detect_shell()
            .await
            .context("failed detecting shell")?;
        log::info!("Remote shell discovered: {}", this.shell);
        this.shell_kind = ShellKind::new(&this.shell, false);
        this.has_wsl_interop = this.detect_has_wsl_interop().await.unwrap_or_default();
        log::info!(
            "Remote has wsl interop {}",
            if this.has_wsl_interop {
                "enabled"
            } else {
                "disabled"
            }
        );
        this.platform = this
            .detect_platform()
            .await
            .context("failed detecting platform")?;
        log::info!("Remote platform discovered: {:?}", this.platform);
        this.os_version = this.detect_os_version().await;
        log::info!("Remote OS version discovered: {:?}", this.os_version);
        this.remote_binary_path = Some(
            this.ensure_server_binary(&delegate, release_channel, version, cx)
                .await
                .context("failed ensuring server binary")?,
        );
        log::debug!("Detected WSL environment: {this:#?}");

        Ok(this)
    }

    async fn detect_platform(&self) -> Result<RemotePlatform> {
        let program = self.shell_kind.prepend_command_prefix("uname");
        let output = self.run_wsl_command_with_output(&program, &["-sm"]).await?;
        parse_platform(&output)
    }

    /// Best-effort detection of the remote OS version for telemetry. Failures
    /// result in `None` rather than failing the connection.
    async fn detect_os_version(&self) -> Option<String> {
        let (program, args) = super::os_version_command(self.platform.os);
        let program = self.shell_kind.prepend_command_prefix(program);
        match self.run_wsl_command_with_output(&program, args).await {
            Ok(output) => super::parse_os_version(self.platform.os, &output),
            Err(error) => {
                log::warn!("Failed to determine remote OS version: {error:#}");
                None
            }
        }
    }

    async fn detect_shell(&self) -> Result<String> {
        const DEFAULT_SHELL: &str = "sh";
        match self
            .run_wsl_command_with_output("sh", &["-c", "echo $SHELL"])
            .await
        {
            Ok(output) => Ok(parse_shell(&output, DEFAULT_SHELL)),
            Err(e) => {
                log::error!("Failed to detect remote shell: {e}");
                Ok(DEFAULT_SHELL.to_owned())
            }
        }
    }

    async fn detect_has_wsl_interop(&self) -> Result<bool> {
        let interop = match self
            .run_wsl_command_with_output("cat", &["/proc/sys/fs/binfmt_misc/WSLInterop"])
            .await
        {
            Ok(interop) => interop,
            Err(err) => self
                .run_wsl_command_with_output("cat", &["/proc/sys/fs/binfmt_misc/WSLInterop-late"])
                .await
                .inspect_err(|err2| log::error!("Failed to detect wsl interop: {err}; {err2}"))?,
        };
        Ok(interop.contains("enabled"))
    }

    async fn windows_path_to_wsl_path(&self, source: &Path) -> Result<String> {
        windows_path_to_wsl_path_impl(&self.connection_options, source).await
    }

    async fn run_wsl_command_with_output(&self, program: &str, args: &[&str]) -> Result<String> {
        run_wsl_command_with_output_impl(&self.connection_options, program, args).await
    }

    async fn run_wsl_command(&self, program: &str, args: &[&str]) -> Result<()> {
        run_wsl_command_impl(wsl_command_impl(
            &self.connection_options,
            program,
            args,
            false,
        ))
        .await
        .map(|_| ())
    }

    async fn ensure_server_binary(
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
            "mav-remote-server-{}-{}",
            release_channel.dev_name(),
            version_str
        );

        let dst_path =
            paths::remote_server_dir_relative().join(RelPath::unix(&binary_name).unwrap());

        if let Some(parent) = dst_path.parent() {
            let parent = parent.display(PathStyle::Posix);
            let mkdir = self.shell_kind.prepend_command_prefix("mkdir");
            self.run_wsl_command(&mkdir, &["-p", &parent])
                .await
                .map_err(|e| anyhow!("Failed to create directory: {}", e))?;
        }

        let binary_exists_on_server = self
            .run_wsl_command(&dst_path.display(PathStyle::Posix), &["version"])
            .await
            .is_ok();

        #[cfg(any(debug_assertions, feature = "build-remote-server-binary"))]
        if let Some(remote_server_path) = super::build_remote_server_from_source(
            &self.platform,
            delegate.as_ref(),
            binary_exists_on_server,
            cx,
        )
        .await?
        {
            let tmp_path = paths::remote_server_dir_relative().join(
                &RelPath::unix(&format!(
                    "download-{}-{}",
                    std::process::id(),
                    remote_server_path.file_name().unwrap().to_string_lossy()
                ))
                .unwrap(),
            );
            self.upload_file(&remote_server_path, &tmp_path, delegate, cx)
                .await?;
            self.extract_and_install(&tmp_path, &dst_path, delegate, cx)
                .await?;
            return Ok(dst_path);
        }

        if binary_exists_on_server {
            return Ok(dst_path);
        }

        let wanted_version = match release_channel {
            ReleaseChannel::Nightly | ReleaseChannel::Dev => None,
            _ => Some(cx.update(|cx| AppVersion::global(cx))),
        };

        let src_path = delegate
            .download_server_binary_locally(self.platform, release_channel, wanted_version, cx)
            .await?;

        let tmp_path = format!(
            "{}.{}.gz",
            dst_path.display(PathStyle::Posix),
            std::process::id()
        );
        let tmp_path = RelPath::unix(&tmp_path).unwrap();

        self.upload_file(&src_path, &tmp_path, delegate, cx).await?;
        self.extract_and_install(&tmp_path, &dst_path, delegate, cx)
            .await?;

        Ok(dst_path)
    }

    async fn upload_file(
        &self,
        src_path: &Path,
        dst_path: &RelPath,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        delegate.set_status(Some("Uploading remote server"), cx);

        if let Some(parent) = dst_path.parent() {
            let parent = parent.display(PathStyle::Posix);
            let mkdir = self.shell_kind.prepend_command_prefix("mkdir");
            self.run_wsl_command(&mkdir, &["-p", &parent])
                .await
                .context("Failed to create directory when uploading file")?;
        }

        let t0 = Instant::now();
        let src_stat = fs::metadata(&src_path)
            .await
            .with_context(|| format!("source path does not exist: {}", src_path.display()))?;
        let size = src_stat.len();
        log::info!(
            "uploading remote server to WSL {:?} ({}kb)",
            dst_path,
            size / 1024
        );

        let src_path_in_wsl = self.windows_path_to_wsl_path(src_path).await?;
        let cp = self.shell_kind.prepend_command_prefix("cp");
        self.run_wsl_command(
            &cp,
            &["-f", &src_path_in_wsl, &dst_path.display(PathStyle::Posix)],
        )
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to copy file {}({}) to WSL {:?}: {}",
                src_path.display(),
                src_path_in_wsl,
                dst_path,
                e
            )
        })?;

        log::info!("uploaded remote server in {:?}", t0.elapsed());
        Ok(())
    }

    async fn extract_and_install(
        &self,
        tmp_path: &RelPath,
        dst_path: &RelPath,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        delegate.set_status(Some("Extracting remote server"), cx);

        let tmp_path_str = tmp_path.display(PathStyle::Posix);
        let dst_path_str = dst_path.display(PathStyle::Posix);

        // Build extraction script with proper error handling
        let script = if tmp_path_str.ends_with(".gz") {
            let uncompressed = tmp_path_str.trim_end_matches(".gz");
            format!(
                "set -e; gunzip -f '{}' && chmod 755 '{}' && mv -f '{}' '{}'",
                tmp_path_str, uncompressed, uncompressed, dst_path_str
            )
        } else {
            format!(
                "set -e; chmod 755 '{}' && mv -f '{}' '{}'",
                tmp_path_str, tmp_path_str, dst_path_str
            )
        };

        self.run_wsl_command("sh", &["-c", &script])
            .await
            .map_err(|e| anyhow!("Failed to extract server binary: {}", e))?;
        Ok(())
    }
}

mod commands;
mod remote_connection;
use commands::{
    run_wsl_command_impl, run_wsl_command_with_output_impl, windows_path_to_wsl_path_impl,
    wsl_command_impl,
};
