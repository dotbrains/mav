use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RemoteConnectionOptions {
    Ssh(SshConnectionOptions),
    Wsl(WslConnectionOptions),
    Docker(DockerConnectionOptions),
    #[cfg(any(test, feature = "test-support"))]
    Mock(crate::transport::mock::MockConnectionOptions),
}

impl RemoteConnectionOptions {
    pub fn display_name(&self) -> String {
        match self {
            RemoteConnectionOptions::Ssh(opts) => opts
                .nickname
                .clone()
                .unwrap_or_else(|| opts.host.to_string()),
            RemoteConnectionOptions::Wsl(opts) => opts.distro_name.clone(),
            RemoteConnectionOptions::Docker(opts) => {
                if opts.use_podman {
                    format!("[podman] {}", opts.name)
                } else {
                    opts.name.clone()
                }
            }
            #[cfg(any(test, feature = "test-support"))]
            RemoteConnectionOptions::Mock(opts) => format!("mock-{}", opts.id),
        }
    }

    /// A stable identifier for the kind of remote connection, suitable for
    /// telemetry (e.g. `"ssh"`, `"wsl"`, `"docker"`, `"podman"`).
    pub fn connection_type(&self) -> &'static str {
        match self {
            RemoteConnectionOptions::Ssh(_) => "ssh",
            RemoteConnectionOptions::Wsl(_) => "wsl",
            RemoteConnectionOptions::Docker(opts) => {
                if opts.use_podman {
                    "podman"
                } else {
                    "docker"
                }
            }
            #[cfg(any(test, feature = "test-support"))]
            RemoteConnectionOptions::Mock(_) => "mock",
        }
    }
}
