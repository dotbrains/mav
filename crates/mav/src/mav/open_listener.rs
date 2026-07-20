use anyhow::{Context as _, Result};
use cli::{CliRequest, CliResponse, CliResponseSink};
use cli::{IpcHandshake, ipc};
use futures::channel::mpsc;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use gpui::Global;
use std::thread;
use util::ResultExt;

mod cli_connection;
pub use cli_connection::handle_cli_connection;

mod open_request;
pub use open_request::{OpenRequest, OpenRequestKind};

mod workspace_opening;
#[cfg(test)]
pub(crate) use workspace_opening::open_local_workspace;
pub(crate) use workspace_opening::open_workspaces;
pub use workspace_opening::{
    derive_paths_with_position, open_options_for_request, open_paths_with_positions,
};

#[derive(Clone)]
pub struct OpenListener(UnboundedSender<RawOpenRequest>);

#[derive(Default)]
pub struct RawOpenRequest {
    pub urls: Vec<String>,
    pub diff_paths: Vec<[String; 2]>,
    pub diff_all: bool,
    pub dev_container: bool,
    pub wsl: Option<String>,
    pub open_behavior: Option<cli::OpenBehavior>,
}

impl Global for OpenListener {}

impl OpenListener {
    pub fn new() -> (Self, UnboundedReceiver<RawOpenRequest>) {
        let (tx, rx) = mpsc::unbounded();
        (OpenListener(tx), rx)
    }

    pub fn open(&self, request: RawOpenRequest) {
        self.0
            .unbounded_send(request)
            .context("no listener for open requests")
            .log_err();
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub fn listen_for_cli_connections(opener: OpenListener) -> Result<()> {
    use release_channel::RELEASE_CHANNEL_NAME;
    use std::os::unix::net::UnixDatagram;

    let sock_path = paths::data_dir().join(format!("mav-{}.sock", *RELEASE_CHANNEL_NAME));
    // remove the socket if the process listening on it has died
    if let Err(e) = UnixDatagram::unbound()?.connect(&sock_path)
        && e.kind() == std::io::ErrorKind::ConnectionRefused
    {
        std::fs::remove_file(&sock_path)?;
    }
    let listener = UnixDatagram::bind(&sock_path)?;
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while let Ok(len) = listener.recv(&mut buf) {
            opener.open(RawOpenRequest {
                urls: vec![String::from_utf8_lossy(&buf[..len]).to_string()],
                ..Default::default()
            });
        }
    });
    Ok(())
}

fn connect_to_cli(
    server_name: &str,
) -> Result<(
    mpsc::UnboundedReceiver<CliRequest>,
    Box<dyn CliResponseSink>,
)> {
    let handshake_tx = ipc::IpcSender::<IpcHandshake>::connect(server_name.to_string())
        .context("error connecting to cli")?;
    let (request_tx, request_rx) = ipc::channel::<CliRequest>()?;
    let (response_tx, response_rx) = ipc::channel::<CliResponse>()?;

    handshake_tx
        .send(IpcHandshake {
            requests: request_tx,
            responses: response_rx,
        })
        .context("error sending ipc handshake")?;

    let (async_request_tx, async_request_rx) = futures::channel::mpsc::unbounded::<CliRequest>();
    thread::spawn(move || {
        while let Ok(cli_request) = request_rx.recv() {
            if async_request_tx.unbounded_send(cli_request).is_err() {
                break;
            }
        }
        anyhow::Ok(())
    });

    Ok((async_request_rx, Box::new(response_tx)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mav::{open_listener::open_local_workspace, tests::init_test};
    use cli::CliResponse;
    use editor::Editor;
    use futures::poll;
    use gpui::{AppContext as _, TestAppContext};
    use language::LineEnding;
    use remote::SshConnectionOptions;
    use rope::Rope;
    use serde_json::json;
    use std::{sync::Arc, task::Poll};
    use util::path;
    use workspace::{AppState, MultiWorkspace};

    struct DiscardResponseSink;

    impl CliResponseSink for DiscardResponseSink {
        fn send(&self, _response: CliResponse) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct SyncResponseSender(std::sync::mpsc::Sender<CliResponse>);

    impl CliResponseSink for SyncResponseSender {
        fn send(&self, response: CliResponse) -> anyhow::Result<()> {
            self.0
                .send(response)
                .map_err(|error| anyhow::anyhow!("{error}"))
        }
    }

    #[path = "parse_tests.rs"]
    mod parse_tests;

    #[path = "dev_container_tests.rs"]
    mod dev_container_tests;

    #[path = "add_flag_tests.rs"]
    mod add_flag_tests;

    #[path = "cli_test_helpers.rs"]
    mod cli_test_helpers;

    #[path = "cli_prompt_tests.rs"]
    mod cli_prompt_tests;

    #[path = "cli_flag_tests.rs"]
    mod cli_flag_tests;

    #[path = "workspace_open_tests.rs"]
    mod workspace_open_tests;
    use workspace_open_tests::open_workspace_file;
}
