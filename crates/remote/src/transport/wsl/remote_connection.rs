use super::*;
use crate::transport::handle_rpc_messages_over_child_process_stdio;

#[async_trait(?Send)]
impl RemoteConnection for WslRemoteConnection {
    fn start_proxy(
        &self,
        unique_identifier: String,
        reconnect: bool,
        incoming_tx: UnboundedSender<Envelope>,
        outgoing_rx: UnboundedReceiver<Envelope>,
        connection_activity_tx: Sender<()>,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Task<Result<i32>> {
        delegate.set_status(Some("Starting proxy"), cx);

        let Some(remote_binary_path) = &self.remote_binary_path else {
            return Task::ready(Err(anyhow!("Remote binary path not set")));
        };

        let mut proxy_args = vec![];
        for env_var in ["RUST_LOG", "RUST_BACKTRACE", "MAV_GENERATE_MINIDUMPS"] {
            if let Some(value) = std::env::var(env_var).ok() {
                // We don't quote the value here as it seems excessive and may result in invalid envs for the
                // proxy server. For example, `RUST_LOG='debug'` will result in a warning "invalid logging spec 'debug'', ignoring it"
                // in the proxy server. Therefore, we pass the env vars as is.
                proxy_args.push(format!("{}={}", env_var, value));
            }
        }

        proxy_args.push(remote_binary_path.display(PathStyle::Posix).into_owned());
        proxy_args.push("proxy".to_owned());
        proxy_args.push("--identifier".to_owned());
        proxy_args.push(unique_identifier);

        if reconnect {
            proxy_args.push("--reconnect".to_owned());
        }

        let proxy_process =
            match wsl_command_impl(&self.connection_options, "env", &proxy_args, true)
                .kill_on_drop(true)
                .spawn()
            {
                Ok(process) => process,
                Err(error) => {
                    return Task::ready(Err(anyhow!("failed to spawn remote server: {}", error)));
                }
            };

        handle_rpc_messages_over_child_process_stdio(
            proxy_process,
            incoming_tx,
            outgoing_rx,
            connection_activity_tx,
            cx,
        )
    }

    fn upload_directory(
        &self,
        src_path: PathBuf,
        dest_path: RemotePathBuf,
        cx: &App,
    ) -> Task<Result<()>> {
        cx.background_spawn({
            let options = self.connection_options.clone();
            async move {
                let wsl_src = windows_path_to_wsl_path_impl(&options, &src_path).await?;
                let command = wsl_command_impl(
                    &options,
                    "cp",
                    &["-r", &wsl_src, &dest_path.to_string()],
                    true,
                );
                run_wsl_command_impl(command).await.map_err(|e| {
                    anyhow!(
                        "failed to upload directory {} -> {}: {}",
                        src_path.display(),
                        dest_path,
                        e
                    )
                })?;

                Ok(())
            }
        })
    }

    async fn kill(&self) -> Result<()> {
        Ok(())
    }

    fn has_been_killed(&self) -> bool {
        false
    }

    fn shares_network_interface(&self) -> bool {
        true
    }

    fn build_command(
        &self,
        program: Option<String>,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<String>,
        port_forward: Option<(u16, String, u16)>,
        _interactive: Interactive,
    ) -> Result<CommandTemplate> {
        if port_forward.is_some() {
            bail!("WSL shares the network interface with the host system");
        }

        let shell_kind = self.shell_kind;
        let working_dir = working_dir
            .map(|working_dir| RemotePathBuf::new(working_dir, PathStyle::Posix).to_string())
            .unwrap_or("~".to_string());

        let mut exec = String::from("exec env ");

        for (key, value) in env.iter() {
            let assignment = format!("{key}={value}");
            let assignment = shell_kind.try_quote(&assignment).context("shell quoting")?;
            write!(exec, "{assignment} ")?;
        }

        if let Some(program) = program {
            write!(
                exec,
                "{}",
                shell_kind
                    .try_quote_prefix_aware(&program)
                    .context("shell quoting")?
            )?;
            for arg in args {
                let arg = shell_kind.try_quote(&arg).context("shell quoting")?;
                write!(exec, " {}", &arg)?;
            }
        } else {
            write!(&mut exec, "{} -l", self.shell)?;
        }
        let (command, args) =
            ShellBuilder::new(&Shell::Program(self.shell.clone()), false).build(Some(exec), &[]);

        let mut wsl_args = if let Some(user) = &self.connection_options.user {
            vec![
                "--distribution".to_string(),
                self.connection_options.distro_name.clone(),
                "--user".to_string(),
                user.clone(),
                "--cd".to_string(),
                working_dir,
                "--".to_string(),
                command,
            ]
        } else {
            vec![
                "--distribution".to_string(),
                self.connection_options.distro_name.clone(),
                "--cd".to_string(),
                working_dir,
                "--".to_string(),
                command,
            ]
        };
        wsl_args.extend(args);

        Ok(CommandTemplate {
            program: "wsl.exe".to_string(),
            args: wsl_args,
            env: HashMap::default(),
        })
    }

    fn build_forward_ports_command(
        &self,
        _: Vec<(u16, String, u16)>,
    ) -> anyhow::Result<CommandTemplate> {
        Err(anyhow!("WSL shares a network interface with the host"))
    }

    fn connection_options(&self) -> RemoteConnectionOptions {
        RemoteConnectionOptions::Wsl(self.connection_options.clone())
    }

    fn path_style(&self) -> PathStyle {
        PathStyle::Posix
    }

    fn remote_platform(&self) -> RemotePlatform {
        self.platform
    }

    fn remote_os_version(&self) -> Option<String> {
        self.os_version.clone()
    }

    fn shell(&self) -> String {
        self.shell.clone()
    }

    fn default_system_shell(&self) -> String {
        self.default_system_shell.clone()
    }

    fn has_wsl_interop(&self) -> bool {
        self.has_wsl_interop
    }
}
