use super::*;

trait CacheableCommand: Any + Send + Sync {
    fn dyn_eq(&self, rhs: &dyn CacheableCommand) -> bool;
    fn dyn_hash(&self, hasher: &mut dyn Hasher);
    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;
}

impl<T> CacheableCommand for T
where
    T: LocalDapCommand + PartialEq + Eq + Hash,
{
    fn dyn_eq(&self, rhs: &dyn CacheableCommand) -> bool {
        (rhs as &dyn Any).downcast_ref::<Self>() == Some(self)
    }

    fn dyn_hash(&self, mut hasher: &mut dyn Hasher) {
        T::hash(self, &mut hasher);
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

pub(crate) struct RequestSlot(Arc<dyn CacheableCommand>);

impl<T: LocalDapCommand + PartialEq + Eq + Hash> From<T> for RequestSlot {
    fn from(request: T) -> Self {
        Self(Arc::new(request))
    }
}

impl PartialEq for RequestSlot {
    fn eq(&self, other: &Self) -> bool {
        self.0.dyn_eq(other.0.as_ref())
    }
}

impl Eq for RequestSlot {}

impl Hash for RequestSlot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.dyn_hash(state);
        (&*self.0 as &dyn Any).type_id().hash(state)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CompletionsQuery {
    pub query: String,
    pub column: u64,
    pub line: Option<u64>,
    pub frame_id: Option<u64>,
}

impl CompletionsQuery {
    pub fn new(
        buffer: &language::Buffer,
        cursor_position: language::Anchor,
        frame_id: Option<u64>,
    ) -> Self {
        let PointUtf16 { row, column } = cursor_position.to_point_utf16(&buffer.snapshot());
        Self {
            query: buffer.text(),
            column: column as u64,
            frame_id,
            line: Some(row as u64),
        }
    }
}

#[derive(Debug)]
pub enum SessionEvent {
    Modules,
    LoadedSources,
    Stopped(Option<ThreadId>),
    StackTrace,
    Variables,
    Watchers,
    Threads,
    InvalidateInlineValue,
    CapabilitiesLoaded,
    RunInTerminal {
        request: RunInTerminalRequestArguments,
        sender: mpsc::Sender<Result<u32>>,
    },
    DataBreakpointInfo,
    ConsoleOutput,
    HistoricSnapshotSelected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStateEvent {
    Running,
    Shutdown,
    Restart,
    SpawnChildSession {
        request: StartDebuggingRequestArguments,
    },
}

impl EventEmitter<SessionEvent> for Session {}
impl EventEmitter<SessionStateEvent> for Session {}
