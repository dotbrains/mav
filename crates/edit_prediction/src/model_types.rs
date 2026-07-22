use super::*;

/// Configuration for using the raw Zeta2 endpoint.
/// When set, the client uses the raw endpoint and constructs the prompt itself.
/// The version is also used as the Baseten environment name (lowercased).
#[derive(Clone)]
pub struct Zeta2RawConfig {
    pub model_id: Option<String>,
    pub environment: Option<String>,
    pub format: ZetaFormat,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EditPredictionModel {
    Zeta,
    Fim { format: EditPredictionPromptFormat },
    Mercury,
}

pub struct EditPredictionModelInput {
    pub(crate) project: Entity<Project>,
    pub(crate) buffer: Entity<Buffer>,
    pub(crate) snapshot: BufferSnapshot,
    pub(crate) position: Anchor,
    pub(crate) events: Vec<Arc<zeta_prompt::Event>>,
    pub(crate) related_files: Vec<RelatedFile>,
    pub(crate) editable_context: Option<Task<anyhow::Result<Vec<RelatedFile>>>>,
    pub(crate) mode: PredictEditsMode,
    pub(crate) trigger: PredictEditsRequestTrigger,
    pub(crate) diagnostic_search_range: Range<Point>,
    pub(crate) debug_tx: Option<mpsc::UnboundedSender<DebugEvent>>,
    pub(crate) can_collect_data: bool,
    pub(crate) is_open_source: bool,
    pub(crate) allow_jump: bool,
}

#[derive(Debug)]
pub enum DebugEvent {
    ContextRetrievalStarted(ContextRetrievalStartedDebugEvent),
    ContextRetrievalFinished(ContextRetrievalFinishedDebugEvent),
    EditPredictionStarted(EditPredictionStartedDebugEvent),
    EditPredictionFinished(EditPredictionFinishedDebugEvent),
}

#[derive(Debug)]
pub struct ContextRetrievalStartedDebugEvent {
    pub project_entity_id: EntityId,
    pub timestamp: Instant,
    pub search_prompt: String,
}

#[derive(Debug)]
pub struct ContextRetrievalFinishedDebugEvent {
    pub project_entity_id: EntityId,
    pub timestamp: Instant,
    pub metadata: Vec<(&'static str, SharedString)>,
}

#[derive(Debug)]
pub struct EditPredictionStartedDebugEvent {
    pub buffer: WeakEntity<Buffer>,
    pub position: Anchor,
    pub prompt: Option<String>,
}

#[derive(Debug)]
pub struct EditPredictionFinishedDebugEvent {
    pub buffer: WeakEntity<Buffer>,
    pub position: Anchor,
    pub model_output: Option<String>,
}
