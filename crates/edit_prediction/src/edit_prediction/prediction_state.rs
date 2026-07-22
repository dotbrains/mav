use super::*;

#[derive(Debug, Clone)]
pub(crate) struct CurrentEditPrediction {
    pub requested_by: EntityId,
    pub prediction: EditPrediction,
    pub was_shown: bool,
    pub shown_with: Option<edit_prediction_types::SuggestionDisplayType>,
    pub e2e_latency: std::time::Duration,
}

impl CurrentEditPrediction {
    fn should_replace_prediction(&self, old_prediction: &Self, cx: &App) -> bool {
        let Some(new_edits) = self
            .prediction
            .interpolate(&self.prediction.buffer.read(cx))
        else {
            return false;
        };

        if self.prediction.buffer != old_prediction.prediction.buffer {
            return true;
        }

        let Some(old_edits) = old_prediction
            .prediction
            .interpolate(&old_prediction.prediction.buffer.read(cx))
        else {
            return true;
        };

        // This reduces the occurrence of UI thrash from replacing edits
        //
        // TODO: This is fairly arbitrary - should have a more general heuristic that handles multiple edits.
        if self.requested_by == self.prediction.buffer.entity_id()
            && self.requested_by == old_prediction.prediction.buffer.entity_id()
            && old_edits.len() == 1
            && new_edits.len() == 1
        {
            let (old_range, old_text) = &old_edits[0];
            let (new_range, new_text) = &new_edits[0];
            new_range == old_range && new_text.starts_with(old_text.as_ref())
        } else {
            true
        }
    }
}

pub(super) const DIAGNOSTIC_LINES_RANGE: u32 = 20;

#[derive(Debug)]
pub(super) struct PendingPrediction {
    id: usize,
    task: Task<Option<(EditPredictionId, Option<String>)>>,
    /// If true, the task is dropped immediately on cancel (cancelling the HTTP request).
    /// If false, the task is awaited to completion so rejection can be reported.
    drop_on_cancel: bool,
}

/// A prediction from the perspective of a buffer.
#[derive(Debug)]
pub(crate) enum BufferEditPrediction<'a> {
    Local { prediction: &'a EditPrediction },
    Jump { prediction: &'a EditPrediction },
}

#[cfg(test)]
impl std::ops::Deref for BufferEditPrediction<'_> {
    type Target = EditPrediction;

    fn deref(&self) -> &Self::Target {
        match self {
            BufferEditPrediction::Local { prediction } => prediction,
            BufferEditPrediction::Jump { prediction } => prediction,
        }
    }
}

pub(super) struct PendingPredictionCapture {
    request_id: EditPredictionId,
    edited_buffer_id: EntityId,
    editable_anchor_range: Range<Anchor>,
    editable_region_before_prediction: String,
    predicted_editable_region: String,
    ts_error_count_before_prediction: usize,
    ts_error_count_after_prediction: usize,
    organization_id: Option<OrganizationId>,
    can_collect_data: bool,
    is_in_open_source_repo: bool,
    sample_data: Option<PendingPredictionCaptureSampleData>,
    model_version: Option<String>,
    enqueued_at: Instant,
    last_edit_at: Instant,
    e2e_latency: std::time::Duration,
}

pub(super) struct PendingPredictionCaptureSampleData {
    context_task: Task<Result<CapturedPredictionContext>>,
    editable_path: Arc<Path>,
    editable_offset_range: Range<usize>,
    next_edit_cursor_offset: Option<usize>,
    future_edit_history_events: Vec<Arc<zeta_prompt::Event>>,
    navigation_history: VecDeque<RecentFile>,
    edit_events_before_quiescence: u32,
    prompt_history_boundary: Option<PromptHistoryBoundary>,
}

/// Marks where the prompt's edit history ended. Sample data may only include
/// content the user produced after this point.
struct PromptHistoryBoundary {
    /// The seq of the first event this capture is expected to observe: the
    /// event that was pending when the prediction was requested, or the next
    /// event to be created if none was pending. Observing a later seq first
    /// means events were lost while the prediction request was in flight.
    first_event_seq: u64,
    /// The prompt's end snapshot within the event that was pending when the
    /// prediction was requested, if any. The first observed event is trimmed
    /// to its suffix after this snapshot.
    snapshot: Option<TextBufferSnapshot>,
}

impl PendingPredictionCapture {
    /// Records the project's last event (pending or finalizing) into this
    /// sample's future edit history. Returns false if the sample must be
    /// dropped because its future history can't be captured accurately.
    fn try_record_future_event(
        &mut self,
        last_event: &LastEvent,
        finalized_event: Option<&StoredEvent>,
        license_detection_watchers: &HashMap<WorktreeId, Rc<LicenseDetectionWatcher>>,
        cx: &App,
    ) {
        let Some(sample) = &mut self.sample_data else {
            return;
        };
        let boundary = sample.prompt_history_boundary.take();
        let suffix_snapshot = match &boundary {
            Some(boundary) => {
                if last_event.seq != boundary.first_event_seq {
                    // Events were finalized before this capture was enqueued,
                    // so events are missing from the future history.
                    self.sample_data.take();
                    return;
                }
                boundary.snapshot.as_ref()
            }
            None => None,
        };

        let event = match suffix_snapshot {
            Some(snapshot) => {
                let suffix = last_event
                    .suffix_after(snapshot)
                    .and_then(|suffix| suffix.finalize(license_detection_watchers, cx));
                let Some(suffix) = suffix else {
                    return;
                };
                suffix.event
            }
            None => match finalized_event {
                Some(event) => event.event.clone(),
                None => return,
            },
        };

        if !event.in_open_source_repo() {
            self.sample_data.take();
            return;
        }
        sample.edit_events_before_quiescence += 1;
        if sample.future_edit_history_events.len() < EDIT_PREDICTION_CAPTURE_MAX_FUTURE_EVENTS {
            sample.future_edit_history_events.push(event);
        }
    }
}
