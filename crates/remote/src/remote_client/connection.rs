use super::*;

#[async_trait(?Send)]
pub trait RemoteConnection: Send + Sync {
    fn start_proxy(
        &self,
        unique_identifier: String,
        reconnect: bool,
        incoming_tx: UnboundedSender<Envelope>,
        outgoing_rx: UnboundedReceiver<Envelope>,
        connection_activity_tx: Sender<()>,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Task<Result<i32>>;
    fn upload_directory(
        &self,
        src_path: PathBuf,
        dest_path: RemotePathBuf,
        cx: &App,
    ) -> Task<Result<()>>;
    async fn kill(&self) -> Result<()>;
    fn has_been_killed(&self) -> bool;
    fn shares_network_interface(&self) -> bool {
        false
    }
    fn build_command(
        &self,
        program: Option<String>,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<String>,
        port_forward: Option<(u16, String, u16)>,
        interactive: Interactive,
    ) -> Result<CommandTemplate>;
    fn build_forward_ports_command(
        &self,
        forwards: Vec<(u16, String, u16)>,
    ) -> Result<CommandTemplate>;
    fn connection_options(&self) -> RemoteConnectionOptions;
    fn path_style(&self) -> PathStyle;
    /// The remote platform (OS and architecture), detected during connection setup.
    fn remote_platform(&self) -> RemotePlatform;
    /// The remote host's OS version (e.g. `"ubuntu 24.04"` or `"15.6.1"`),
    /// detected during connection setup. `None` if it could not be determined.
    fn remote_os_version(&self) -> Option<String>;
    fn shell(&self) -> String;
    fn default_system_shell(&self) -> String;
    fn has_wsl_interop(&self) -> bool;

    #[cfg(any(test, feature = "test-support"))]
    fn simulate_disconnect(&self, _: &AsyncApp) {}
}

pub(super) type ResponseChannels =
    Mutex<HashMap<MessageId, oneshot::Sender<(Envelope, oneshot::Sender<()>)>>>;
pub(super) type StreamResponseChannels =
    Arc<Mutex<HashMap<MessageId, UnboundedSender<(Result<Envelope>, oneshot::Sender<()>)>>>>;
