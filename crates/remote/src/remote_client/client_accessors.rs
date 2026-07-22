use super::*;

impl RemoteClient {
    pub fn shell(&self) -> Option<String> {
        Some(self.remote_connection()?.shell())
    }

    pub fn default_system_shell(&self) -> Option<String> {
        Some(self.remote_connection()?.default_system_shell())
    }

    pub fn shares_network_interface(&self) -> bool {
        self.remote_connection()
            .map_or(false, |connection| connection.shares_network_interface())
    }

    pub fn has_wsl_interop(&self) -> bool {
        self.remote_connection()
            .map_or(false, |connection| connection.has_wsl_interop())
    }

    pub fn build_command(
        &self,
        program: Option<String>,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<String>,
        port_forward: Option<(u16, String, u16)>,
        interactive: Interactive,
    ) -> Result<CommandTemplate> {
        let Some(connection) = self.remote_connection() else {
            return Err(anyhow!("no remote connection"));
        };
        connection.build_command(program, args, env, working_dir, port_forward, interactive)
    }

    pub fn build_forward_ports_command(
        &self,
        forwards: Vec<(u16, String, u16)>,
    ) -> Result<CommandTemplate> {
        let Some(connection) = self.remote_connection() else {
            return Err(anyhow!("no remote connection"));
        };
        connection.build_forward_ports_command(forwards)
    }

    pub fn upload_directory(
        &self,
        src_path: PathBuf,
        dest_path: RemotePathBuf,
        cx: &App,
    ) -> Task<Result<()>> {
        let Some(connection) = self.remote_connection() else {
            return Task::ready(Err(anyhow!("no remote connection")));
        };
        connection.upload_directory(src_path, dest_path, cx)
    }

    pub fn proto_client(&self) -> AnyProtoClient {
        self.client.clone().into()
    }

    pub fn connection_options(&self) -> RemoteConnectionOptions {
        self.connection_options.clone()
    }

    pub fn connection(&self) -> Option<Arc<dyn RemoteConnection>> {
        if let State::Connected {
            remote_connection, ..
        } = self.state.as_ref()?
        {
            Some(remote_connection.clone())
        } else {
            None
        }
    }

    pub fn connection_state(&self) -> ConnectionState {
        self.state
            .as_ref()
            .map(ConnectionState::from)
            .unwrap_or(ConnectionState::Disconnected)
    }

    pub fn is_disconnected(&self) -> bool {
        self.connection_state() == ConnectionState::Disconnected
    }

    pub fn path_style(&self) -> PathStyle {
        self.path_style
    }

    /// The platform (OS and architecture) of the remote host, detected during
    /// connection setup.
    pub fn remote_platform(&self) -> RemotePlatform {
        self.platform
    }

    /// The OS version of the remote host (e.g. `"ubuntu 24.04"`), detected
    /// during connection setup. `None` if it could not be determined.
    pub fn remote_os_version(&self) -> Option<String> {
        self.os_version.clone()
    }

    /// A stable identifier for the kind of remote connection (e.g. `"ssh"`,
    /// `"wsl"`, `"docker"`, `"podman"`).
    pub fn connection_type(&self) -> &'static str {
        self.connection_options.connection_type()
    }

    /// Forcibly disconnects from the remote server by killing the underlying connection.
    /// This will trigger the reconnection logic if reconnection attempts remain.
    /// Useful for testing reconnection behavior in real environments.
    pub fn force_disconnect(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let Some(connection) = self.remote_connection() else {
            return Task::ready(Err(anyhow!("no active remote connection to disconnect")));
        };

        log::info!("force_disconnect: killing remote connection");

        cx.spawn(async move |_, _| {
            connection.kill().await?;
            Ok(())
        })
    }

    /// Simulates a timeout by pausing heartbeat responses.
    /// This will cause heartbeat failures and eventually trigger reconnection
    /// after MAX_MISSED_HEARTBEATS are missed.
    /// Useful for testing timeout behavior in real environments.
    pub fn force_heartbeat_timeout(&mut self, attempts: usize, cx: &mut Context<Self>) {
        log::info!("force_heartbeat_timeout: triggering heartbeat failure state");

        if let Some(State::Connected {
            remote_connection,
            delegate,
            multiplex_task,
            heartbeat_task,
        }) = self.state.take()
        {
            self.set_state(
                if attempts == 0 {
                    State::HeartbeatMissed {
                        missed_heartbeats: MAX_MISSED_HEARTBEATS,
                        remote_connection,
                        delegate,
                        multiplex_task,
                        heartbeat_task,
                    }
                } else {
                    State::ReconnectFailed {
                        remote_connection,
                        delegate,
                        error: anyhow!("forced heartbeat timeout"),
                        attempts,
                    }
                },
                cx,
            );

            self.reconnect(cx)
                .context("failed to start reconnect after forced timeout")
                .log_err();
        } else {
            log::warn!("force_heartbeat_timeout: not in Connected state, ignoring");
        }
    }
}
