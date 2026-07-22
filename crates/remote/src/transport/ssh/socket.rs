use super::*;

impl SshSocket {
    #[cfg(not(windows))]
    pub(super) async fn new(options: SshConnectionOptions, socket_path: PathBuf) -> Result<Self> {
        Ok(Self {
            connection_options: options,
            envs: HashMap::default(),
            socket_path,
        })
    }

    #[cfg(windows)]
    pub(super) async fn new(
        options: SshConnectionOptions,
        password: askpass::EncryptedPassword,
        executor: gpui::BackgroundExecutor,
    ) -> Result<Self> {
        let mut envs = HashMap::default();
        let get_password =
            move |_| Task::ready(std::ops::ControlFlow::Continue(Ok(password.clone())));

        let _proxy = askpass::PasswordProxy::new(Box::new(get_password), executor).await?;
        envs.insert("SSH_ASKPASS_REQUIRE".into(), "force".into());
        envs.insert(
            "SSH_ASKPASS".into(),
            _proxy.script_path().as_ref().display().to_string(),
        );
        envs.insert(
            "MAV_ASKPASS_SOCKET".into(),
            _proxy.socket_path().as_ref().display().to_string(),
        );

        Ok(Self {
            connection_options: options,
            envs,
            _proxy,
        })
    }

    // :WARNING: ssh unquotes arguments when executing on the remote :WARNING:
    // e.g. $ ssh host sh -c 'ls -l' is equivalent to $ ssh host sh -c ls -l
    // and passes -l as an argument to sh, not to ls.
    // Furthermore, some setups (e.g. Coder) will change directory when SSH'ing
    // into a machine. You must use `cd` to get back to $HOME.
    // You need to do it like this: $ ssh host "cd; sh -c 'ls -l /tmp'"
    pub(super) fn ssh_command(
        &self,
        shell_kind: ShellKind,
        program: &str,
        args: &[impl AsRef<str>],
        allow_pseudo_tty: bool,
    ) -> util::command::Command {
        let mut command = util::command::new_command("ssh");
        let program = shell_kind.prepend_command_prefix(program);
        let mut to_run = shell_kind
            .try_quote_prefix_aware(&program)
            .expect("shell quoting")
            .into_owned();
        for arg in args {
            // We're trying to work with: sh, bash, zsh, fish, tcsh, ...?
            debug_assert!(
                !arg.as_ref().contains('\n'),
                "multiline arguments do not work in all shells"
            );
            to_run.push(' ');
            to_run.push_str(&shell_kind.try_quote(arg.as_ref()).expect("shell quoting"));
        }
        let to_run = if shell_kind == ShellKind::Cmd {
            to_run // 'cd' prints the current directory in CMD
        } else {
            let separator = shell_kind.sequential_commands_separator();
            format!("cd{separator} {to_run}")
        };
        self.ssh_options(&mut command, true)
            .arg(self.connection_options.ssh_destination());
        if !allow_pseudo_tty {
            command.arg("-T");
        }
        command.arg(to_run);
        log::debug!("ssh {:?}", command);
        command
    }

    pub(super) async fn run_command(
        &self,
        shell_kind: ShellKind,
        program: &str,
        args: &[impl AsRef<str>],
        allow_pseudo_tty: bool,
    ) -> Result<String> {
        let mut command = self.ssh_command(shell_kind, program, args, allow_pseudo_tty);
        let output = command.output().await?;
        log::debug!("{:?}: {:?}", command, output);
        anyhow::ensure!(
            output.status.success(),
            "failed to run command {command:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub(super) fn ssh_options<'a>(
        &self,
        command: &'a mut util::command::Command,
        include_port_forwards: bool,
    ) -> &'a mut util::command::Command {
        let args = if include_port_forwards {
            self.connection_options.additional_args()
        } else {
            self.connection_options.additional_args_for_scp()
        };

        let cmd = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(args);

        if cfg!(windows) {
            cmd.envs(self.envs.clone());
        }
        #[cfg(not(windows))]
        {
            cmd.args(["-o", "ControlMaster=no", "-o"])
                .arg(format!("ControlPath={}", self.socket_path.display()));
        }
        cmd
    }

    // Returns the SSH command-line options (without the destination) for building commands.
    // On Linux, this includes the ControlPath option to reuse the existing connection.
    // Note: The destination must be added separately after all options to ensure proper
    // SSH command structure: ssh [options] destination [command]
    pub(super) fn ssh_command_options(&self) -> Vec<String> {
        let arguments = self.connection_options.additional_args();
        #[cfg(not(windows))]
        let arguments = {
            let mut args = arguments;
            args.extend(vec![
                "-o".to_string(),
                "ControlMaster=no".to_string(),
                "-o".to_string(),
                format!("ControlPath={}", self.socket_path.display()),
            ]);
            args
        };
        arguments
    }

    pub(super) async fn platform(
        &self,
        shell: ShellKind,
        is_windows: bool,
    ) -> Result<RemotePlatform> {
        if is_windows {
            self.platform_windows(shell).await
        } else {
            self.platform_posix(shell).await
        }
    }

    async fn platform_posix(&self, shell: ShellKind) -> Result<RemotePlatform> {
        let output = self
            .run_command(shell, "uname", &["-sm"], false)
            .await
            .context("Failed to run 'uname -sm' to determine platform")?;
        parse_platform(&output)
    }

    /// Best-effort detection of the remote OS version. Failures are logged and
    /// result in `None` rather than failing the connection, since this is only
    /// used for telemetry.
    pub(super) async fn os_version(&self, os: RemoteOs, shell: ShellKind) -> Option<String> {
        let (program, args) = crate::transport::os_version_command(os);
        match self.run_command(shell, program, args, false).await {
            Ok(output) => crate::transport::parse_os_version(os, &output),
            Err(error) => {
                log::warn!("Failed to determine remote OS version: {error:#}");
                None
            }
        }
    }

    async fn platform_windows(&self, shell: ShellKind) -> Result<RemotePlatform> {
        let output = self
            .run_command(
                shell,
                "cmd.exe",
                &["/c", "echo", "%PROCESSOR_ARCHITECTURE%"],
                false,
            )
            .await
            .context(
                "Failed to run 'echo %PROCESSOR_ARCHITECTURE%' to determine Windows architecture",
            )?;

        Ok(RemotePlatform {
            os: RemoteOs::Windows,
            arch: match output.trim() {
                "AMD64" => RemoteArch::X86_64,
                "ARM64" => RemoteArch::Aarch64,
                arch => anyhow::bail!(
                    "Prebuilt remote servers are not yet available for windows-{arch}. See https://mav.dev/docs/remote-development"
                ),
            },
        })
    }

    /// Probes whether the remote host is running Windows.
    ///
    /// This is done by attempting to run a simple Windows-specific command.
    /// If it succeeds and returns Windows-like output, we assume it's Windows.
    pub(super) async fn probe_is_windows(&self) -> bool {
        match self
            .run_command(ShellKind::Cmd, "cmd.exe", &["/c", "ver"], false)
            .await
        {
            // Windows 'ver' command outputs something like "Microsoft Windows [Version 10.0.19045.5011]"
            Ok(output) => output.trim().contains("indows"),
            Err(_) => false,
        }
    }

    pub(super) async fn shell(&self, is_windows: bool) -> String {
        if is_windows {
            self.shell_windows().await
        } else {
            self.shell_posix().await
        }
    }

    async fn shell_posix(&self) -> String {
        const DEFAULT_SHELL: &str = "sh";
        match self
            .run_command(ShellKind::Posix, "sh", &["-c", "echo $SHELL"], false)
            .await
        {
            Ok(output) => parse_shell(&output, DEFAULT_SHELL),
            Err(e) => {
                log::error!("Failed to detect remote shell: {e}");
                DEFAULT_SHELL.to_owned()
            }
        }
    }

    async fn shell_windows(&self) -> String {
        const DEFAULT_SHELL: &str = "cmd.exe";

        // We detect the shell used by the SSH session by running the following command in PowerShell:
        // (Get-CimInstance Win32_Process -Filter "ProcessId = $((Get-CimInstance Win32_Process -Filter ProcessId=$PID).ParentProcessId)").Name
        // This prints the name of PowerShell's parent process (which will be the shell that SSH launched).
        // We pass it as a Base64 encoded string since we don't yet know how to correctly quote that command.
        // (We'd need to know what the shell is to do that...)
        match self
            .run_command(
                ShellKind::Cmd,
                "powershell",
                &[
                    "-E",
                    "KABHAGUAdAAtAEMAaQBtAEkAbgBzAHQAYQBuAGMAZQAgAFcAaQBuADMAMgBfAFAAcgBvAGMAZQBzAHMAIAAtAEYAaQBsAHQAZQByACAAIgBQAHIAbwBjAGUAcwBzAEkAZAAgAD0AIAAkACgAKABHAGUAdAAtAEMAaQBtAEkAbgBzAHQAYQBuAGMAZQAgAFcAaQBuADMAMgBfAFAAcgBvAGMAZQBzAHMAIAAtAEYAaQBsAHQAZQByACAAUAByAG8AYwBlAHMAcwBJAGQAPQAkAFAASQBEACkALgBQAGEAcgBlAG4AdABQAHIAbwBjAGUAcwBzAEkAZAApACIAKQAuAE4AYQBtAGUA",
                ],
                false,
            )
            .await
        {
            Ok(output) => parse_shell(&output, DEFAULT_SHELL),
            Err(e) => {
                log::error!("Failed to detect remote shell: {e}");
                DEFAULT_SHELL.to_owned()
            }
        }
    }
}
