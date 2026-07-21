use super::*;

impl ToolCallEventStream {
    pub(super) fn new(
        tool_use_id: LanguageModelToolUseId,
        stream: ThreadEventStream,
        fs: Option<Arc<dyn Fs>>,
        cancellation_rx: watch::Receiver<bool>,
        sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
        thread: Option<WeakEntity<Thread>>,
    ) -> Self {
        Self {
            tool_use_id,
            stream,
            fs,
            cancellation_rx,
            sandbox_grants,
            thread,
        }
    }

    /// Whether the owning thread is a subagent, so prompts can say "for this
    /// subagent" instead of "for this thread".
    pub(super) fn is_subagent(&self, cx: &App) -> bool {
        self.thread
            .as_ref()
            .and_then(|thread| thread.upgrade())
            .is_some_and(|thread| thread.read(cx).is_subagent())
    }

    /// Persist the thread so a freshly recorded "for this thread" sandbox grant
    /// survives a reopen. Saving is driven by the agent's `observe` on the
    /// thread entity, so a no-op `notify` is enough to schedule it.
    pub(super) fn persist_thread_grants(thread: &Option<WeakEntity<Thread>>, cx: &AsyncApp) {
        let Some(thread) = thread else { return };
        cx.update(|cx| {
            thread.update(cx, |_thread, cx| cx.notify()).ok();
        });
    }
}
