use super::*;

pub(super) struct RunningTurn {
    /// Holds the task that handles agent interaction until the end of the turn.
    /// Survives across multiple requests as the model performs tool calls and
    /// we run tools, report their results.
    pub(super) _task: Task<()>,
    /// The current event stream for the running turn. Used to report a final
    /// cancellation event if we cancel the turn.
    pub(super) event_stream: ThreadEventStream,
    /// The tools that are enabled for the current iteration of the turn.
    /// Refreshed at the start of each iteration via `refresh_turn_tools`.
    pub(super) tools: BTreeMap<SharedString, Arc<dyn AnyAgentTool>>,
    /// Sender to signal tool cancellation. When cancel is called, this is
    /// set to true so all tools can detect user-initiated cancellation.
    pub(super) cancellation_tx: watch::Sender<bool>,
    /// Senders for tools that support input streaming and have already been
    /// started but are still receiving input from the LLM.
    pub(super) streaming_tool_inputs: HashMap<LanguageModelToolUseId, ToolInputSender>,
}

impl RunningTurn {
    pub(super) fn new(
        event_stream: ThreadEventStream,
        tools: BTreeMap<SharedString, Arc<dyn AnyAgentTool>>,
        cancellation_tx: watch::Sender<bool>,
        task: Task<()>,
    ) -> Self {
        Self {
            _task: task,
            event_stream,
            tools,
            cancellation_tx,
            streaming_tool_inputs: HashMap::default(),
        }
    }

    pub(super) fn cancel(mut self) -> Task<()> {
        log::debug!("Cancelling in progress turn");
        self.cancellation_tx.send(true).ok();
        self.event_stream.send_canceled();
        self._task
    }
}
