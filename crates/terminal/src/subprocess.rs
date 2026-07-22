use super::{PtyEvent, TerminalBackendEvent};
use crate::alacritty::AlacrittyTermLock;
use anyhow::Result;
use collections::HashMap;
use gpui::{BackgroundExecutor, Task};
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;
use util::ResultExt as _;
use vte::ansi::{Processor, StdSyncHandler};

/// Converts bare LFs into CRLFs so output captured from a pipe (rather than a
/// PTY) wraps correctly in Alacritty. A PTY's line discipline performs this
/// `ONLCR` translation for us; piped output (e.g. `ls` run outside a PTY) only
/// emits `\n`, which moves Alacritty's cursor down without returning it to
/// column zero and makes the rendered output look misaligned. Alacritty has no
/// setting for this, so we insert a `\r` before each `\n` that lacks one.
pub(super) fn convert_lf_to_crlf(bytes: &[u8], previous_byte_was_cr: &mut bool) -> Vec<u8> {
    let mut converted = Vec::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == b'\n' && !*previous_byte_was_cr {
            converted.push(b'\r');
        }
        converted.push(byte);
        *previous_byte_was_cr = byte == b'\r';
    }
    converted
}

/// Owns a non-PTY task subprocess and the background task pumping its output
/// into the terminal emulator. Used by headless hosts (e.g. the eval CLI) where
/// PTY allocation fails with `ENOTTY`. Dropping this kills the child.
pub(super) struct SubprocessHandle {
    child: Arc<parking_lot::Mutex<Option<util::process::Child>>>,
    _reader: Task<()>,
}

impl SubprocessHandle {
    pub(super) fn kill(&self) {
        if let Some(child) = self.child.lock().as_mut() {
            child.kill().log_err();
        }
    }
}

/// Spawns `program`/`args` as a plain subprocess with piped stdout/stderr and
/// drives its output into `term`, mirroring what the Alacritty event loop does
/// for a PTY but without one. Used when [`HeadlessTerminal`] is enabled.
pub(super) fn spawn_task_subprocess(
    program: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    working_directory: Option<PathBuf>,
    term: Arc<AlacrittyTermLock>,
    events_tx: futures::channel::mpsc::UnboundedSender<PtyEvent>,
    executor: &BackgroundExecutor,
) -> Result<SubprocessHandle> {
    use futures::io::AsyncReadExt as _;
    use std::process::Stdio;

    let mut command = util::command::new_std_command(&program);
    command.args(&args);
    command.envs(&env);
    if let Some(directory) = &working_directory {
        command.current_dir(directory);
    }

    let mut child =
        util::process::Child::spawn(command, Stdio::null(), Stdio::piped(), Stdio::piped())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(parking_lot::Mutex::new(Some(child)));

    let reader = executor.spawn({
        let child = child.clone();
        let executor = executor.clone();
        async move {
            // stdout and stderr are pumped concurrently, each through its own
            // parser; the shared term mutex serializes grid mutation.
            type BoxedReader = Box<dyn futures::io::AsyncRead + Unpin + Send>;
            let pump = |reader: Option<BoxedReader>| {
                let term = term.clone();
                let events_tx = events_tx.clone();
                async move {
                    let Some(mut reader) = reader else { return };
                    let mut processor = Processor::<StdSyncHandler>::new();
                    let mut buffer = [0u8; 8192];
                    let mut previous_byte_was_cr = false;
                    loop {
                        match reader.read(&mut buffer).await {
                            Ok(0) => return,
                            Err(error) => {
                                log::warn!("failed to read subprocess output: {error}");
                                return;
                            }
                            Ok(count) => {
                                let converted =
                                    convert_lf_to_crlf(&buffer[..count], &mut previous_byte_was_cr);
                                {
                                    let mut term = term.lock();
                                    processor.advance(&mut *term, &converted);
                                }
                                events_tx
                                    .unbounded_send(PtyEvent::Event(TerminalBackendEvent::Wakeup))
                                    .ok();
                            }
                        }
                    }
                }
            };
            let stdout = stdout.map(|reader| Box::new(reader) as BoxedReader);
            let stderr = stderr.map(|reader| Box::new(reader) as BoxedReader);
            futures::future::join(pump(stdout), pump(stderr)).await;

            // Both pipes are closed, so the child has exited or is about to.
            // Poll for its status without holding the lock across an await.
            let status = loop {
                let status = match child.lock().as_mut() {
                    Some(child) => match child.try_status() {
                        Ok(status) => status,
                        Err(error) => {
                            log::warn!("failed to get subprocess exit status: {error}");
                            break None;
                        }
                    },
                    None => Some(ExitStatus::default()),
                };
                match status {
                    Some(status) => break Some(status),
                    None => executor.timer(Duration::from_millis(20)).await,
                }
            };
            child.lock().take();
            let event = match status {
                Some(status) => TerminalBackendEvent::ChildExit(status),
                None => TerminalBackendEvent::Exit,
            };
            events_tx.unbounded_send(PtyEvent::Event(event)).ok();
        }
    });

    Ok(SubprocessHandle {
        child,
        _reader: reader,
    })
}
