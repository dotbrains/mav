use super::*;

pub(crate) struct FakeTerminalHandle {
    pub(crate) killed: Arc<AtomicBool>,
    stopped_by_user: Arc<AtomicBool>,
    exit_sender: std::cell::RefCell<Option<futures::channel::oneshot::Sender<()>>>,
    wait_for_exit: Shared<Task<acp::TerminalExitStatus>>,
    output: acp::TerminalOutputResponse,
    id: acp::TerminalId,
}

impl FakeTerminalHandle {
    pub(crate) fn new_never_exits(cx: &mut App) -> Self {
        let killed = Arc::new(AtomicBool::new(false));
        let stopped_by_user = Arc::new(AtomicBool::new(false));

        let (exit_sender, exit_receiver) = futures::channel::oneshot::channel();

        let wait_for_exit = cx
            .spawn(async move |_cx| {
                let _ = exit_receiver.await;
                acp::TerminalExitStatus::new()
            })
            .shared();

        Self {
            killed,
            stopped_by_user,
            exit_sender: std::cell::RefCell::new(Some(exit_sender)),
            wait_for_exit,
            output: acp::TerminalOutputResponse::new("partial output".to_string(), false),
            id: acp::TerminalId::new("fake_terminal".to_string()),
        }
    }

    pub(crate) fn new_with_immediate_exit(cx: &mut App, exit_code: u32) -> Self {
        let killed = Arc::new(AtomicBool::new(false));
        let stopped_by_user = Arc::new(AtomicBool::new(false));
        let (exit_sender, _exit_receiver) = futures::channel::oneshot::channel();

        let wait_for_exit = cx
            .spawn(async move |_cx| acp::TerminalExitStatus::new().exit_code(exit_code))
            .shared();

        Self {
            killed,
            stopped_by_user,
            exit_sender: std::cell::RefCell::new(Some(exit_sender)),
            wait_for_exit,
            output: acp::TerminalOutputResponse::new("command output".to_string(), false),
            id: acp::TerminalId::new("fake_terminal".to_string()),
        }
    }

    pub(crate) fn with_output(mut self, output: acp::TerminalOutputResponse) -> Self {
        self.output = output;
        self
    }

    pub(crate) fn was_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    pub(crate) fn set_stopped_by_user(&self, stopped: bool) {
        self.stopped_by_user.store(stopped, Ordering::SeqCst);
    }

    pub(crate) fn signal_exit(&self) {
        if let Some(sender) = self.exit_sender.borrow_mut().take() {
            let _ = sender.send(());
        }
    }
}

impl crate::TerminalHandle for FakeTerminalHandle {
    fn id(&self, _cx: &AsyncApp) -> Result<acp::TerminalId> {
        Ok(self.id.clone())
    }

    fn current_output(&self, _cx: &AsyncApp) -> Result<acp::TerminalOutputResponse> {
        Ok(self.output.clone())
    }

    fn wait_for_exit(&self, _cx: &AsyncApp) -> Result<Shared<Task<acp::TerminalExitStatus>>> {
        Ok(self.wait_for_exit.clone())
    }

    fn kill(&self, _cx: &AsyncApp) -> Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        self.signal_exit();
        Ok(())
    }

    fn was_stopped_by_user(&self, _cx: &AsyncApp) -> Result<bool> {
        Ok(self.stopped_by_user.load(Ordering::SeqCst))
    }
}

pub(crate) struct FakeSubagentHandle {
    session_id: acp::SessionId,
    send_task: Shared<Task<String>>,
}

impl SubagentHandle for FakeSubagentHandle {
    fn id(&self) -> acp::SessionId {
        self.session_id.clone()
    }

    fn num_entries(&self, _cx: &App) -> usize {
        unimplemented!()
    }

    fn send(&self, _message: String, cx: &AsyncApp) -> Task<Result<String>> {
        let task = self.send_task.clone();
        cx.background_spawn(async move { Ok(task.await) })
    }
}

#[derive(Default)]
pub(crate) struct FakeThreadEnvironment {
    pub(crate) terminal_handle: Option<Rc<FakeTerminalHandle>>,
    pub(crate) subagent_handle: Option<Rc<FakeSubagentHandle>>,
    terminal_creations: Arc<AtomicUsize>,
    terminal_output_limits: std::cell::RefCell<Vec<Option<u64>>>,
}

impl FakeThreadEnvironment {
    pub(crate) fn with_terminal(self, terminal_handle: FakeTerminalHandle) -> Self {
        Self {
            terminal_handle: Some(terminal_handle.into()),
            ..self
        }
    }

    pub(crate) fn terminal_creation_count(&self) -> usize {
        self.terminal_creations.load(Ordering::SeqCst)
    }

    pub(crate) fn terminal_output_limits(&self) -> Vec<Option<u64>> {
        self.terminal_output_limits.borrow().clone()
    }
}

impl crate::ThreadEnvironment for FakeThreadEnvironment {
    fn create_terminal(
        &self,
        _command: String,
        _extra_env: Vec<acp::EnvVariable>,
        _cwd: Option<std::path::PathBuf>,
        output_byte_limit: Option<u64>,
        _sandbox_wrap: Option<acp_thread::SandboxWrap>,
        _cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn crate::TerminalHandle>>> {
        self.terminal_creations.fetch_add(1, Ordering::SeqCst);
        self.terminal_output_limits
            .borrow_mut()
            .push(output_byte_limit);
        let handle = self
            .terminal_handle
            .clone()
            .expect("Terminal handle not available on FakeThreadEnvironment");
        Task::ready(Ok(handle as Rc<dyn crate::TerminalHandle>))
    }

    fn create_subagent(&self, _label: String, _cx: &mut App) -> Result<Rc<dyn SubagentHandle>> {
        Ok(self
            .subagent_handle
            .clone()
            .expect("Subagent handle not available on FakeThreadEnvironment")
            as Rc<dyn SubagentHandle>)
    }
}

pub(crate) struct MultiTerminalEnvironment {
    handles: std::cell::RefCell<Vec<Rc<FakeTerminalHandle>>>,
}

impl MultiTerminalEnvironment {
    pub(crate) fn new() -> Self {
        Self {
            handles: std::cell::RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn handles(&self) -> Vec<Rc<FakeTerminalHandle>> {
        self.handles.borrow().clone()
    }
}

impl crate::ThreadEnvironment for MultiTerminalEnvironment {
    fn create_terminal(
        &self,
        _command: String,
        _extra_env: Vec<acp::EnvVariable>,
        _cwd: Option<std::path::PathBuf>,
        _output_byte_limit: Option<u64>,
        _sandbox_wrap: Option<acp_thread::SandboxWrap>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn crate::TerminalHandle>>> {
        let handle = Rc::new(cx.update(|cx| FakeTerminalHandle::new_never_exits(cx)));
        self.handles.borrow_mut().push(handle.clone());
        Task::ready(Ok(handle as Rc<dyn crate::TerminalHandle>))
    }

    fn create_subagent(&self, _label: String, _cx: &mut App) -> Result<Rc<dyn SubagentHandle>> {
        unimplemented!()
    }
}
