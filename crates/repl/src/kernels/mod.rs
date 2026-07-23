mod native_kernel;
use std::{fmt::Debug, path::PathBuf};

use futures::{channel::mpsc, future::Shared};
use gpui::{App, Entity, Task, Window};
use log;
pub use native_kernel::*;

mod python_env;
pub use python_env::*;

mod remote_kernels;
pub use remote_kernels::*;

mod ssh_kernel;
pub use ssh_kernel::*;

mod wsl_kernel;
pub use wsl_kernel::*;

use anyhow::Result;
use futures::{FutureExt, StreamExt};
use gpui::{AppContext, AsyncWindowContext, Context};
use jupyter_protocol::{JupyterKernelspec, JupyterMessageContent};
use runtimelib::{
    ClientControlConnection, ClientIoPubConnection, ClientShellConnection, ClientStdinConnection,
    ExecutionState, JupyterMessage, KernelInfoReply,
};
use ui::{Icon, IconName, SharedString};

pub fn start_kernel_tasks<S: KernelSession + 'static>(
    session: Entity<S>,
    iopub_socket: ClientIoPubConnection,
    shell_socket: ClientShellConnection,
    control_socket: ClientControlConnection,
    stdin_socket: ClientStdinConnection,
    cx: &mut AsyncWindowContext,
) -> (
    futures::channel::mpsc::Sender<JupyterMessage>,
    futures::channel::mpsc::Sender<JupyterMessage>,
) {
    let (mut shell_send, shell_recv) = shell_socket.split();
    let (mut control_send, control_recv) = control_socket.split();
    let (mut stdin_send, stdin_recv) = stdin_socket.split();

    let (request_tx, mut request_rx) = futures::channel::mpsc::channel::<JupyterMessage>(100);
    let (stdin_tx, mut stdin_rx) = futures::channel::mpsc::channel::<JupyterMessage>(100);

    let recv_task = cx.spawn({
        let session = session.clone();
        let mut iopub = iopub_socket;
        let mut shell = shell_recv;
        let mut control = control_recv;
        let mut stdin = stdin_recv;

        async move |cx| -> anyhow::Result<()> {
            loop {
                let (channel, result) = futures::select! {
                    msg = iopub.read().fuse() => ("iopub", msg),
                    msg = shell.read().fuse() => ("shell", msg),
                    msg = control.read().fuse() => ("control", msg),
                    msg = stdin.read().fuse() => ("stdin", msg),
                };
                match result {
                    Ok(message) => {
                        session
                            .update_in(cx, |session, window, cx| {
                                session.route(&message, window, cx);
                            })
                            .ok();
                    }
                    Err(
                        ref err @ (runtimelib::RuntimeError::ParseError { .. }
                        | runtimelib::RuntimeError::SerdeError(_)),
                    ) => {
                        let error_detail = format!("Kernel issue on {channel} channel\n\n{err}");
                        log::warn!("kernel: {error_detail}");
                        session
                            .update_in(cx, |session, _window, cx| {
                                session.kernel_errored(error_detail, cx);
                                cx.notify();
                            })
                            .ok();
                    }
                    Err(err) => {
                        log::warn!("kernel: error reading from {channel}: {err:?}");
                        anyhow::bail!("{channel} recv: {err}");
                    }
                }
            }
        }
    });

    let routing_task = cx.background_spawn(async move {
        while let Some(message) = request_rx.next().await {
            match message.content {
                JupyterMessageContent::DebugRequest(_)
                | JupyterMessageContent::InterruptRequest(_)
                | JupyterMessageContent::ShutdownRequest(_) => {
                    control_send.send(message).await?;
                }
                _ => {
                    shell_send.send(message).await?;
                }
            }
        }
        anyhow::Ok(())
    });

    let stdin_routing_task = cx.background_spawn(async move {
        while let Some(message) = stdin_rx.next().await {
            stdin_send.send(message).await?;
        }
        anyhow::Ok(())
    });

    cx.spawn({
        async move |cx| {
            async fn with_name(
                name: &'static str,
                task: Task<Result<()>>,
            ) -> (&'static str, Result<()>) {
                (name, task.await)
            }

            let mut tasks = futures::stream::FuturesUnordered::new();
            tasks.push(with_name("recv task", recv_task));
            tasks.push(with_name("routing task", routing_task));
            tasks.push(with_name("stdin routing task", stdin_routing_task));

            while let Some((name, result)) = tasks.next().await {
                if let Err(err) = result {
                    session.update(cx, |session, cx| {
                        session.kernel_errored(format!("handling failed for {name}: {err}"), cx);
                        cx.notify();
                    });
                }
            }
        }
    })
    .detach();

    (request_tx, stdin_tx)
}

pub trait KernelSession: Sized {
    fn route(&mut self, message: &JupyterMessage, window: &mut Window, cx: &mut Context<Self>);
    fn kernel_errored(&mut self, error_message: String, cx: &mut Context<Self>);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelSpecification {
    JupyterServer(RemoteKernelSpecification),
    Jupyter(LocalKernelSpecification),
    PythonEnv(PythonEnvKernelSpecification),
    SshRemote(SshRemoteKernelSpecification),
    WslRemote(WslKernelSpecification),
}

#[derive(Debug, Clone)]
pub struct SshRemoteKernelSpecification {
    pub name: String,
    pub path: SharedString,
    pub kernelspec: JupyterKernelspec,
}

#[derive(Debug, Clone)]
pub struct WslKernelSpecification {
    pub name: String,
    pub kernelspec: JupyterKernelspec,
    pub distro: String,
}

impl PartialEq for SshRemoteKernelSpecification {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.kernelspec.argv == other.kernelspec.argv
            && self.path == other.path
            && self.kernelspec.display_name == other.kernelspec.display_name
            && self.kernelspec.language == other.kernelspec.language
            && self.kernelspec.interrupt_mode == other.kernelspec.interrupt_mode
            && self.kernelspec.env == other.kernelspec.env
            && self.kernelspec.metadata == other.kernelspec.metadata
    }
}

impl Eq for SshRemoteKernelSpecification {}

impl PartialEq for WslKernelSpecification {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.kernelspec.argv == other.kernelspec.argv
            && self.kernelspec.display_name == other.kernelspec.display_name
            && self.kernelspec.language == other.kernelspec.language
            && self.kernelspec.interrupt_mode == other.kernelspec.interrupt_mode
            && self.kernelspec.env == other.kernelspec.env
            && self.kernelspec.metadata == other.kernelspec.metadata
            && self.distro == other.distro
    }
}

impl Eq for WslKernelSpecification {}

impl KernelSpecification {
    pub fn name(&self) -> SharedString {
        match self {
            Self::Jupyter(spec) => spec.name.clone().into(),
            Self::PythonEnv(spec) => spec.name.clone().into(),
            Self::JupyterServer(spec) => spec.name.clone().into(),
            Self::SshRemote(spec) => spec.name.clone().into(),
            Self::WslRemote(spec) => spec.kernelspec.display_name.clone().into(),
        }
    }

    pub fn type_name(&self) -> SharedString {
        match self {
            Self::Jupyter(_) => "Jupyter".into(),
            Self::PythonEnv(spec) => SharedString::from(
                spec.environment_kind
                    .clone()
                    .unwrap_or_else(|| "Python Environment".to_string()),
            ),
            Self::JupyterServer(_) => "Jupyter Server".into(),
            Self::SshRemote(_) => "SSH Remote".into(),
            Self::WslRemote(_) => "WSL Remote".into(),
        }
    }

    pub fn path(&self) -> SharedString {
        SharedString::from(match self {
            Self::Jupyter(spec) => spec.path.to_string_lossy().into_owned(),
            Self::PythonEnv(spec) => spec.path.to_string_lossy().into_owned(),
            Self::JupyterServer(spec) => spec.url.to_string(),
            Self::SshRemote(spec) => spec.path.to_string(),
            Self::WslRemote(spec) => spec.distro.clone(),
        })
    }

    pub fn language(&self) -> SharedString {
        SharedString::from(match self {
            Self::Jupyter(spec) => spec.kernelspec.language.clone(),
            Self::PythonEnv(spec) => spec.kernelspec.language.clone(),
            Self::JupyterServer(spec) => spec.kernelspec.language.clone(),
            Self::SshRemote(spec) => spec.kernelspec.language.clone(),
            Self::WslRemote(spec) => spec.kernelspec.language.clone(),
        })
    }

    pub fn has_ipykernel(&self) -> bool {
        match self {
            Self::Jupyter(_) | Self::JupyterServer(_) | Self::SshRemote(_) | Self::WslRemote(_) => {
                true
            }
            Self::PythonEnv(spec) => spec.has_ipykernel,
        }
    }

    pub fn environment_kind_label(&self) -> Option<SharedString> {
        match self {
            Self::PythonEnv(spec) => spec
                .environment_kind
                .as_ref()
                .map(|kind| SharedString::from(kind.clone())),
            Self::Jupyter(_) => Some("Jupyter".into()),
            Self::JupyterServer(_) => Some("Jupyter Server".into()),
            Self::SshRemote(_) => Some("SSH Remote".into()),
            Self::WslRemote(_) => Some("WSL Remote".into()),
        }
    }

    pub fn icon(&self, cx: &App) -> Icon {
        let lang_name = match self {
            Self::Jupyter(spec) => spec.kernelspec.language.clone(),
            Self::PythonEnv(spec) => spec.kernelspec.language.clone(),
            Self::JupyterServer(spec) => spec.kernelspec.language.clone(),
            Self::SshRemote(spec) => spec.kernelspec.language.clone(),
            Self::WslRemote(spec) => spec.kernelspec.language.clone(),
        };

        file_icons::FileIcons::get(cx)
            .get_icon_for_type(&lang_name.to_lowercase(), cx)
            .map(Icon::from_path)
            .unwrap_or(Icon::new(IconName::ReplNeutral))
    }
}

pub trait RunningKernel: Send + Debug {
    fn request_tx(&self) -> mpsc::Sender<JupyterMessage>;
    fn stdin_tx(&self) -> mpsc::Sender<JupyterMessage>;
    fn working_directory(&self) -> &PathBuf;
    fn execution_state(&self) -> &ExecutionState;
    fn set_execution_state(&mut self, state: ExecutionState);
    fn kernel_info(&self) -> Option<&KernelInfoReply>;
    fn set_kernel_info(&mut self, info: KernelInfoReply);
    fn force_shutdown(&mut self, window: &mut Window, cx: &mut App) -> Task<anyhow::Result<()>>;
    fn kill(&mut self);
}

#[derive(Debug, Clone)]
pub enum KernelStatus {
    Idle,
    Busy,
    Starting,
    Error,
    ShuttingDown,
    Shutdown,
    Restarting,
}

impl KernelStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, KernelStatus::Idle | KernelStatus::Busy)
    }
}

impl ToString for KernelStatus {
    fn to_string(&self) -> String {
        match self {
            KernelStatus::Idle => "Idle".to_string(),
            KernelStatus::Busy => "Busy".to_string(),
            KernelStatus::Starting => "Starting".to_string(),
            KernelStatus::Error => "Error".to_string(),
            KernelStatus::ShuttingDown => "Shutting Down".to_string(),
            KernelStatus::Shutdown => "Shutdown".to_string(),
            KernelStatus::Restarting => "Restarting".to_string(),
        }
    }
}

#[derive(Debug)]
pub enum Kernel {
    RunningKernel(Box<dyn RunningKernel>),
    StartingKernel(Shared<Task<()>>),
    ErroredLaunch(String),
    ShuttingDown,
    Shutdown,
    Restarting,
}

impl From<&Kernel> for KernelStatus {
    fn from(kernel: &Kernel) -> Self {
        match kernel {
            Kernel::RunningKernel(kernel) => match kernel.execution_state() {
                ExecutionState::Idle => KernelStatus::Idle,
                ExecutionState::Busy => KernelStatus::Busy,
                ExecutionState::Unknown => KernelStatus::Error,
                ExecutionState::Starting => KernelStatus::Starting,
                ExecutionState::Restarting => KernelStatus::Restarting,
                ExecutionState::Terminating => KernelStatus::ShuttingDown,
                ExecutionState::AutoRestarting => KernelStatus::Restarting,
                ExecutionState::Dead => KernelStatus::Error,
                ExecutionState::Other(_) => KernelStatus::Error,
            },
            Kernel::StartingKernel(_) => KernelStatus::Starting,
            Kernel::ErroredLaunch(_) => KernelStatus::Error,
            Kernel::ShuttingDown => KernelStatus::ShuttingDown,
            Kernel::Shutdown => KernelStatus::Shutdown,
            Kernel::Restarting => KernelStatus::Restarting,
        }
    }
}

impl Kernel {
    pub fn status(&self) -> KernelStatus {
        self.into()
    }

    pub fn set_execution_state(&mut self, status: &ExecutionState) {
        if let Kernel::RunningKernel(running_kernel) = self {
            running_kernel.set_execution_state(status.clone());
        }
    }

    pub fn set_kernel_info(&mut self, kernel_info: &KernelInfoReply) {
        if let Kernel::RunningKernel(running_kernel) = self {
            running_kernel.set_kernel_info(kernel_info.clone());
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        match self {
            Kernel::Restarting | Kernel::ShuttingDown => true,
            Kernel::RunningKernel(_)
            | Kernel::StartingKernel(_)
            | Kernel::ErroredLaunch(_)
            | Kernel::Shutdown => false,
        }
    }
}
