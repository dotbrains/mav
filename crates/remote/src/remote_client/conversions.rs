use super::*;

impl From<SshConnectionOptions> for RemoteConnectionOptions {
    fn from(opts: SshConnectionOptions) -> Self {
        RemoteConnectionOptions::Ssh(opts)
    }
}

impl From<WslConnectionOptions> for RemoteConnectionOptions {
    fn from(opts: WslConnectionOptions) -> Self {
        RemoteConnectionOptions::Wsl(opts)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl From<crate::transport::mock::MockConnectionOptions> for RemoteConnectionOptions {
    fn from(opts: crate::transport::mock::MockConnectionOptions) -> Self {
        RemoteConnectionOptions::Mock(opts)
    }
}

#[cfg(target_os = "windows")]
/// Open a wsl path (\\wsl.localhost\<distro>\path)
#[derive(Debug, Clone, PartialEq, Eq, gpui::Action)]
#[action(namespace = workspace, no_json, no_register)]
pub struct OpenWslPath {
    pub distro: WslConnectionOptions,
    pub paths: Vec<PathBuf>,
}
