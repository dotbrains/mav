use anyhow::{Context as _, Result, anyhow};
use buffer_diff::BufferDiff;
use client::{Client, EditPredictionUsage, UserStore, global_llm_token};
use cloud_api_client::LlmApiToken;
use cloud_api_types::{
    EditPredictionRecentFile, EditPredictionSettledKeptChars,
    MAX_EDIT_PREDICTION_SETTLED_PER_REQUEST, OrganizationId, SettledEditPrediction,
    SettledEditPredictionSampleData, SubmitEditPredictionFeedbackBody,
    SubmitEditPredictionSettledBatchBody, SubmitEditPredictionSettledResponse,
};
use cloud_llm_client::predict_edits_v3::{
    PREDICT_EDITS_MODE_HEADER_NAME, PREDICT_EDITS_REQUEST_ID_HEADER_NAME,
    PREDICT_EDITS_TRIGGER_HEADER_NAME, PredictEditsMode, PredictEditsV3Request,
    PredictEditsV3Response, RawCompletionRequest, RawCompletionResponse,
};
use cloud_llm_client::predict_edits_v4::{PredictEditsV4Request, PredictEditsV4Response};
use cloud_llm_client::{
    EditPredictionRejectReason, EditPredictionRejection, MAV_VERSION_HEADER_NAME,
    MAX_EDIT_PREDICTION_REJECTIONS_PER_REQUEST, MINIMUM_REQUIRED_VERSION_HEADER_NAME,
    PREFERRED_EXPERIMENT_HEADER_NAME, PredictEditsRequestTrigger, RejectEditPredictionsBodyRef,
};
use collections::{HashMap, HashSet};
use copilot::{Copilot, Reinstall, SignIn, SignOut};
use credentials_provider::CredentialsProvider;
use db::kvp::{Dismissable, KeyValueStore};
use edit_prediction_context::{RelatedExcerptStore, RelatedExcerptStoreEvent, RelatedFile};
use edit_prediction_types::EditPredictionRequestTrigger;
use feature_flags::{FeatureFlag, FeatureFlagAppExt as _, PresenceFlag, register_feature_flag};
use futures::{
    AsyncReadExt as _, FutureExt as _, StreamExt as _,
    channel::mpsc::{self, UnboundedReceiver},
    select_biased,
};
use git::repository::FileHistoryChangedFileSets;
use gpui::BackgroundExecutor;
use gpui::TaskExt;
use gpui::http_client::Url;
use gpui::{
    App, AsyncApp, Context, Entity, EntityId, Global, SharedString, Task, WeakEntity, actions,
    http_client::{self, AsyncBody, Method},
    prelude::*,
};
use heapless::Vec as ArrayVec;
use language::{
    Anchor, Buffer, BufferEditSource, BufferSnapshot, EditPredictionPromptFormat,
    EditPredictionsMode, EditPreview, File, OffsetRangeExt, Point, TextBufferSnapshot, ToOffset,
    ToPoint, language_settings::all_language_settings,
};
use project::{DisableAiSettings, Project, ProjectPath, WorktreeId};
use release_channel::AppVersion;
use semver::Version;
use serde::de::DeserializeOwned;
use settings::{
    EditPredictionDataCollectionChoice, EditPredictionProvider, Settings as _, update_settings_file,
};
use std::collections::{VecDeque, hash_map};
use std::env;
use std::rc::Rc;
use text::{AnchorRangeExt, Edit};
use workspace::{AppState, Workspace};
use zeta_prompt::ContextSource;
use zeta_prompt::{Zeta2PromptInput, Zeta3PromptInput, ZetaFormat};

use std::mem;
use std::ops::Range;
use std::path::Path;
use std::str::FromStr as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;
use util::ResultExt as _;

#[path = "edit_prediction/buffer_predictions.rs"]
mod buffer_predictions;
#[path = "edit_prediction/context_data.rs"]
mod context_data;
pub mod cursor_excerpt;
pub mod data_collection;
mod edit_ranges;
#[path = "edit_prediction/event_helpers.rs"]
mod event_helpers;
pub mod example_spec;
#[path = "edit_prediction/feedback_workers.rs"]
mod feedback_workers;
pub mod fim;
#[path = "edit_prediction/last_event.rs"]
mod last_event;
mod license_detection;
pub mod mercury;
pub mod metrics;
mod model_types;
pub mod ollama;
mod onboarding_modal;
pub mod open_ai_response;
#[path = "edit_prediction/path_helpers.rs"]
mod path_helpers;
mod prediction;
#[path = "edit_prediction/prediction_refresh.rs"]
mod prediction_refresh;
#[path = "edit_prediction/prediction_state.rs"]
mod prediction_state;
#[path = "edit_prediction/project_integration.rs"]
mod project_integration;
#[path = "edit_prediction/project_state.rs"]
mod project_state;
#[path = "edit_prediction/refresh_entry.rs"]
mod refresh_entry;
#[path = "edit_prediction/request_pipeline.rs"]
mod request_pipeline;
#[path = "edit_prediction/settled_helpers.rs"]
mod settled_helpers;
#[path = "edit_prediction/store_config.rs"]
mod store_config;
#[path = "edit_prediction/store_helpers.rs"]
mod store_helpers;
#[path = "edit_prediction/stored_event.rs"]
mod stored_event;

pub mod udiff;

mod mav_edit_prediction_delegate;
pub mod open_ai_compatible;
pub mod zeta;

#[cfg(test)]
mod edit_prediction_tests;

use crate::cursor_excerpt::expand_context_syntactically_then_linewise;
use crate::data_collection::{CapturedPredictionContext, capture_prediction_context};
use crate::edit_ranges::{
    compute_diff_between_snapshots_in_range, compute_total_edit_range_between_snapshots,
    merge_anchor_ranges,
};
pub(crate) use crate::event_helpers::{lines_between_ranges, push_recent_file};
use crate::example_spec::RecentFile;
use crate::license_detection::LicenseDetectionWatcher;
use crate::mercury::Mercury;
pub use crate::metrics::{KeptRateResult, compute_kept_rate};
use crate::onboarding_modal::MavPredictModal;
pub(crate) use crate::path_helpers::{
    buffer_path_with_id_fallback, predict_edits_request_trigger_from_editor_trigger,
};
use crate::prediction::EditPredictionResult;
pub use crate::prediction::{EditPrediction, EditPredictionId, EditPredictionInputs};
use crate::prediction_state::*;
pub(crate) use crate::settled_helpers::{
    currently_following, is_ep_store_provider, send_settled_batches,
};
pub(crate) use crate::store_helpers::{
    CloudRequestTimeoutError, MavPredictUpsell, MavUpdateRequiredError,
    collaborator_edit_overlaps_locality_region, is_mav_industries_repo,
    merge_trailing_events_if_needed,
};
pub use language_model::ApiKeyState;
pub use mav_edit_prediction_delegate::MavEditPredictionDelegate;
pub use telemetry_events::EditPredictionRating;

actions!(
    edit_prediction,
    [
        /// Resets the edit prediction onboarding state.
        ResetOnboarding,
        /// Clears the edit prediction history.
        ClearHistory,
    ]
);

/// Maximum number of events to track.
const EVENT_COUNT_MAX: usize = 10;
const RECENT_PATH_COUNT_MAX: usize = 20;
const CHANGE_GROUPING_LINE_SPAN: u32 = 8;
const EDIT_HISTORY_DIFF_SIZE_LIMIT: usize = 2048 * 3; // ~2048 tokens or ~50% of typical prompt budget
const COLLABORATOR_EDIT_LOCALITY_CONTEXT_TOKENS: usize = 512;
const GIT_CHANGED_FILE_SETS_COMMIT_LIMIT: usize = 100;
const LAST_CHANGE_GROUPING_TIME: Duration = Duration::from_secs(1);
const MAV_PREDICT_DATA_COLLECTION_CHOICE: &str = "mav_predict_data_collection_choice";
const REJECT_REQUEST_DEBOUNCE: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT_BACKOFF: Duration = Duration::from_secs(10);

const EDIT_PREDICTION_SETTLED_TTL: Duration = Duration::from_secs(60 * 5);
const EDIT_PREDICTION_SETTLED_QUIESCENCE: Duration = Duration::from_secs(10);
const EDIT_PREDICTION_CAPTURE_MAX_FUTURE_EVENTS: usize = 4;
const EDIT_PREDICTION_SETTLED_MAX_EDITABLE_REGION_BYTES: usize = 4 * 1024;

pub struct EditPredictionJumpsFeatureFlag;

impl FeatureFlag for EditPredictionJumpsFeatureFlag {
    const NAME: &'static str = "edit_prediction_jumps";
    type Value = PresenceFlag;
}
register_feature_flag!(EditPredictionJumpsFeatureFlag);

#[derive(Clone)]
struct EditPredictionStoreGlobal(Entity<EditPredictionStore>);

impl Global for EditPredictionStoreGlobal {}

pub use model_types::{
    ContextRetrievalFinishedDebugEvent, ContextRetrievalStartedDebugEvent, DebugEvent,
    EditPredictionFinishedDebugEvent, EditPredictionModel, EditPredictionModelInput,
    EditPredictionStartedDebugEvent, Zeta2RawConfig,
};

pub struct EditPredictionStore {
    client: Arc<Client>,
    user_store: Entity<UserStore>,
    llm_token: LlmApiToken,
    _fetch_experiments_task: Task<()>,
    projects: HashMap<EntityId, ProjectState>,
    update_required: bool,
    edit_prediction_model: EditPredictionModel,
    zeta2_raw_config: Option<Zeta2RawConfig>,
    request_backoff_until: Option<Instant>,
    preferred_experiment: Option<String>,
    available_experiments: Vec<String>,
    pub mercury: Mercury,
    legacy_data_collection_enabled: bool,
    reject_predictions_tx: mpsc::UnboundedSender<EditPredictionRejectionPayload>,
    settled_predictions_tx: mpsc::UnboundedSender<Instant>,
    rateable_predictions: VecDeque<EditPrediction>,
    rated_predictions: HashSet<EditPredictionId>,
    #[cfg(test)]
    settled_event_callback: Option<Box<dyn Fn(EditPredictionId, String)>>,
    credentials_provider: Arc<dyn CredentialsProvider>,
}

pub(crate) struct EditPredictionRejectionPayload {
    rejection: EditPredictionRejection,
    organization_id: Option<OrganizationId>,
}

/// An event with associated metadata for reconstructing buffer state.
#[derive(Clone)]
pub struct StoredEvent {
    pub event: Arc<zeta_prompt::Event>,
    pub old_snapshot: TextBufferSnapshot,
    pub new_snapshot_version: clock::Global,
    pub total_edit_range: Range<Anchor>,
    pub(crate) file_context: Option<Entity<StoredFileContext>>,
}

pub(crate) struct StoredFileContext {
    pub(crate) uncommitted_diff: Option<Entity<BufferDiff>>,
    pub(crate) git_changed_file_sets: Option<Arc<FileHistoryChangedFileSets>>,
    pub(crate) git_changed_file_sets_task: Option<Task<()>>,
}

struct ProjectState {
    events: VecDeque<StoredEvent>,
    last_event: Option<LastEvent>,
    next_last_event_seq: u64,
    recently_viewed_files: VecDeque<RecentFile>,
    recently_opened_files: VecDeque<RecentFile>,
    registered_buffers: HashMap<gpui::EntityId, RegisteredBuffer>,
    file_contexts: HashMap<ProjectPath, WeakEntity<StoredFileContext>>,
    current_prediction: Option<CurrentEditPrediction>,
    last_edit_source: Option<BufferEditSource>,
    next_pending_prediction_id: usize,
    pending_predictions: ArrayVec<PendingPrediction, 2, u8>,
    pending_prediction_captures: Vec<PendingPredictionCapture>,
    debug_tx: Option<mpsc::UnboundedSender<DebugEvent>>,
    last_edit_prediction_refresh: Option<(EntityId, Instant)>,
    cancelled_predictions: HashSet<usize>,
    context: Entity<RelatedExcerptStore>,
    license_detection_watchers: HashMap<WorktreeId, Rc<LicenseDetectionWatcher>>,
    _subscriptions: [gpui::Subscription; 2],
    copilot: Option<Entity<Copilot>>,
}

struct RegisteredBuffer {
    file: Option<Arc<dyn File>>,
    snapshot: TextBufferSnapshot,
    last_position: Option<Anchor>,
    _subscriptions: [gpui::Subscription; 2],
}

#[derive(Clone)]
struct LastEvent {
    /// Project-wide monotonic sequence number identifying this event.
    seq: u64,
    old_snapshot: TextBufferSnapshot,
    new_snapshot: TextBufferSnapshot,
    old_file: Option<Arc<dyn File>>,
    new_file: Option<Arc<dyn File>>,
    latest_edit_range: Range<Anchor>,
    total_edit_range: Range<Anchor>,
    total_edit_range_at_last_pause_boundary: Option<Range<Anchor>>,
    predicted: bool,
    snapshot_after_last_editing_pause: Option<TextBufferSnapshot>,
    last_edit_time: Option<Instant>,
    file_context: Option<Entity<StoredFileContext>>,
}
