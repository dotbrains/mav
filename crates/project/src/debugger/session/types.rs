use super::*;

#[derive(Debug, Copy, Clone, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(transparent)]
pub struct ThreadId(pub i64);

impl From<i64> for ThreadId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

#[derive(Clone, Debug)]
pub struct StackFrame {
    pub dap: dap::StackFrame,
    pub scopes: Vec<dap::Scope>,
}

impl From<dap::StackFrame> for StackFrame {
    fn from(stack_frame: dap::StackFrame) -> Self {
        Self {
            scopes: vec![],
            dap: stack_frame,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ThreadStatus {
    #[default]
    Running,
    Stopped,
    Stepping,
    Exited,
    Ended,
}

impl ThreadStatus {
    pub fn label(&self) -> &'static str {
        match self {
            ThreadStatus::Running => "Running",
            ThreadStatus::Stopped => "Stopped",
            ThreadStatus::Stepping => "Stepping",
            ThreadStatus::Exited => "Exited",
            ThreadStatus::Ended => "Ended",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Thread {
    dap: dap::Thread,
    stack_frames: Vec<StackFrame>,
    stack_frames_error: Option<SharedString>,
    _has_stopped: bool,
}

impl From<dap::Thread> for Thread {
    fn from(dap: dap::Thread) -> Self {
        Self {
            dap,
            stack_frames: Default::default(),
            stack_frames_error: None,
            _has_stopped: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Watcher {
    pub expression: SharedString,
    pub value: SharedString,
    pub variables_reference: u64,
    pub presentation_hint: Option<VariablePresentationHint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataBreakpointState {
    pub dap: dap::DataBreakpoint,
    pub is_enabled: bool,
    pub context: Arc<DataBreakpointContext>,
}

pub enum SessionState {
    /// Represents a session that is building/initializing
    /// even if a session doesn't have a pre build task this state
    /// is used to run all the async tasks that are required to start the session
    Booting(Option<Task<Result<()>>>),
    Running(RunningMode),
}

impl SessionState {
    pub(super) fn request_dap<R: LocalDapCommand>(&self, request: R) -> Task<Result<R::Response>>
    where
        <R::DapRequest as dap::requests::Request>::Response: 'static,
        <R::DapRequest as dap::requests::Request>::Arguments: 'static + Send,
    {
        match self {
            SessionState::Running(debug_adapter_client) => debug_adapter_client.request(request),
            SessionState::Booting(_) => Task::ready(Err(anyhow!(
                "no adapter running to send request: {request:?}"
            ))),
        }
    }

    /// Did this debug session stop at least once?
    pub(crate) fn has_ever_stopped(&self) -> bool {
        match self {
            SessionState::Booting(_) => false,
            SessionState::Running(running_mode) => running_mode.has_ever_stopped,
        }
    }

    fn stopped(&mut self) {
        if let SessionState::Running(running) = self {
            running.has_ever_stopped = true;
        }
    }
}

#[derive(Default)]
struct ThreadStates {
    global_state: Option<ThreadStatus>,
    known_thread_states: IndexMap<ThreadId, ThreadStatus>,
}

impl ThreadStates {
    fn stop_all_threads(&mut self) {
        self.global_state = Some(ThreadStatus::Stopped);
        self.known_thread_states.clear();
    }

    fn exit_all_threads(&mut self) {
        self.global_state = Some(ThreadStatus::Exited);
        self.known_thread_states.clear();
    }

    fn continue_all_threads(&mut self) {
        self.global_state = Some(ThreadStatus::Running);
        self.known_thread_states.clear();
    }

    fn stop_thread(&mut self, thread_id: ThreadId) {
        self.known_thread_states
            .insert(thread_id, ThreadStatus::Stopped);
    }

    fn continue_thread(&mut self, thread_id: ThreadId) {
        self.known_thread_states
            .insert(thread_id, ThreadStatus::Running);
    }

    fn process_step(&mut self, thread_id: ThreadId) {
        self.known_thread_states
            .insert(thread_id, ThreadStatus::Stepping);
    }

    fn thread_status(&self, thread_id: ThreadId) -> ThreadStatus {
        self.thread_state(thread_id)
            .unwrap_or(ThreadStatus::Running)
    }

    fn thread_state(&self, thread_id: ThreadId) -> Option<ThreadStatus> {
        self.known_thread_states
            .get(&thread_id)
            .copied()
            .or(self.global_state)
    }

    fn exit_thread(&mut self, thread_id: ThreadId) {
        self.known_thread_states
            .insert(thread_id, ThreadStatus::Exited);
    }

    fn any_stopped_thread(&self) -> bool {
        self.global_state
            .is_some_and(|state| state == ThreadStatus::Stopped)
            || self
                .known_thread_states
                .values()
                .any(|status| *status == ThreadStatus::Stopped)
    }
}

// TODO(debugger): Wrap dap types with reference counting so the UI doesn't have to clone them on refresh
#[derive(Default)]
pub struct SessionSnapshot {
    threads: IndexMap<ThreadId, Thread>,
    thread_states: ThreadStates,
    variables: HashMap<VariableReference, Vec<dap::Variable>>,
    stack_frames: IndexMap<StackFrameId, StackFrame>,
    locations: HashMap<u64, dap::LocationsResponse>,
    modules: Vec<dap::Module>,
    loaded_sources: Vec<dap::Source>,
}

type IsEnabled = bool;

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct OutputToken(pub usize);
