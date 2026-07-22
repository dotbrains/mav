use super::*;
use crate::transport::ssh::control_master::find_existing_control_master;

impl SshRemoteConnection {
    pub(crate) async fn new(
        connection_options: SshConnectionOptions,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        use askpass::AskPassResult;

        let destination = connection_options.ssh_destination();

        let temp_dir = tempfile::Builder::new()
            .prefix("mav-ssh-session")
            .tempdir()?;

        // On non-Windows, check if the user already has an active ControlMaster
        // session for this host. If so, reuse it instead of prompting for auth.
        #[cfg(not(windows))]
        let reused_socket =
            find_existing_control_master(&destination, &connection_options.additional_args()).await;

        #[cfg(not(windows))]
        let (socket, master_process_option) = if let Some(reused_path) = reused_socket {
            delegate.set_status(Some("Connecting (reusing session)"), cx);
            log::info!("reusing existing ControlMaster, skipping authentication");
            let socket = SshSocket::new(connection_options, reused_path).await?;
            (socket, None)
        } else {
            let askpass_delegate = askpass::AskPassDelegate::new(cx, {
                let delegate = delegate.clone();
                move |prompt, tx, cx| delegate.ask_password(prompt, tx, cx)
            });

            let mut askpass =
                askpass::AskPassSession::new(cx.background_executor().clone(), askpass_delegate)
                    .await?;

            delegate.set_status(Some("Connecting"), cx);

            // Start the master SSH process, which does not do anything except
            // for establish the connection and keep it open, allowing other ssh
            // commands to reuse it via a control socket.
            let socket_path = temp_dir.path().join("ssh.sock");
            let mut master_process = MasterProcess::new(
                askpass.script_path().as_ref(),
                connection_options.additional_args(),
                &socket_path,
                &destination,
            )?;

            let result = select_biased! {
                result = askpass.run().fuse() => {
                    match result {
                        AskPassResult::CancelledByUser => {
                            master_process.as_mut().kill().ok();
                            anyhow::bail!("SSH connection canceled")
                        }
                        AskPassResult::Timedout => {
                            anyhow::bail!("connecting to host timed out")
                        }
                    }
                }
                _ = master_process.wait_connected().fuse() => {
                    anyhow::Ok(())
                }
            };

            if let Err(e) = result {
                return Err(e.context("Failed to connect to host"));
            }

            if master_process.as_mut().try_status()?.is_some() {
                let mut output = Vec::new();
                let mut stderr = master_process.as_mut().stderr.take().unwrap();
                stderr.read_to_end(&mut output).await?;

                let error_message = format!(
                    "failed to connect: {}",
                    String::from_utf8_lossy(&output).trim()
                );
                anyhow::bail!(error_message);
            }

            let socket = SshSocket::new(connection_options, socket_path).await?;
            drop(askpass);
            (socket, Some(master_process))
        };

        #[cfg(windows)]
        let (socket, master_process_option) = {
            let askpass_delegate = askpass::AskPassDelegate::new(cx, {
                let delegate = delegate.clone();
                move |prompt, tx, cx| delegate.ask_password(prompt, tx, cx)
            });

            let mut askpass =
                askpass::AskPassSession::new(cx.background_executor().clone(), askpass_delegate)
                    .await?;

            delegate.set_status(Some("Connecting"), cx);

            let mut master_process = MasterProcess::new(
                askpass.script_path().as_ref(),
                askpass.socket_path().as_ref(),
                connection_options.additional_args(),
                &destination,
            )?;

            let result = select_biased! {
                result = askpass.run().fuse() => {
                    match result {
                        AskPassResult::CancelledByUser => {
                            master_process.as_mut().kill().ok();
                            anyhow::bail!("SSH connection canceled")
                        }
                        AskPassResult::Timedout => {
                            anyhow::bail!("connecting to host timed out")
                        }
                    }
                }
                _ = master_process.wait_connected().fuse() => {
                    anyhow::Ok(())
                }
            };

            if let Err(e) = result {
                return Err(e.context("Failed to connect to host"));
            }

            if master_process.as_mut().try_status()?.is_some() {
                let mut output = Vec::new();
                let mut stderr = master_process.as_mut().stderr.take().unwrap();
                stderr.read_to_end(&mut output).await?;

                let error_message = format!(
                    "failed to connect: {}",
                    String::from_utf8_lossy(&output).trim()
                );
                anyhow::bail!(error_message);
            }

            let socket = SshSocket::new(
                connection_options,
                askpass
                    .get_password()
                    .or_else(|| askpass::EncryptedPassword::try_from("").ok())
                    .context("Failed to fetch askpass password")?,
                cx.background_executor().clone(),
            )
            .await?;
            drop(askpass);

            (socket, Some(master_process))
        };

        let is_windows = socket.probe_is_windows().await;
        log::info!("Remote is windows: {}", is_windows);

        let ssh_shell = socket.shell(is_windows).await;
        log::info!("Remote shell discovered: {}", ssh_shell);

        let ssh_shell_kind = ShellKind::new(&ssh_shell, is_windows);
        let ssh_platform = socket.platform(ssh_shell_kind, is_windows).await?;
        log::info!("Remote platform discovered: {:?}", ssh_platform);

        let ssh_os_version = socket.os_version(ssh_platform.os, ssh_shell_kind).await;
        log::info!("Remote OS version discovered: {:?}", ssh_os_version);

        let (ssh_path_style, ssh_default_system_shell) = match ssh_platform.os {
            RemoteOs::Windows => (PathStyle::Windows, ssh_shell.clone()),
            _ => (PathStyle::Posix, String::from("/bin/sh")),
        };

        let mut this = Self {
            socket,
            master_process: Mutex::new(master_process_option),
            killed: AtomicBool::new(false),
            _temp_dir: temp_dir,
            remote_binary_path: None,
            ssh_path_style,
            ssh_platform,
            ssh_os_version,
            ssh_shell,
            ssh_shell_kind,
            ssh_default_system_shell,
        };

        let (release_channel, version) =
            cx.update(|cx| (ReleaseChannel::global(cx), AppVersion::global(cx)));
        this.remote_binary_path = Some(
            this.ensure_server_binary(&delegate, release_channel, version, cx)
                .await?,
        );

        Ok(this)
    }
}
