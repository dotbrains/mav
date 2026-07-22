use super::*;

pub(crate) fn buffer_path_with_id_fallback(
    file: Option<&Arc<dyn File>>,
    snapshot: &TextBufferSnapshot,
    cx: &App,
) -> Arc<Path> {
    if let Some(file) = file {
        file.full_path(cx).into()
    } else {
        Path::new(&format!("untitled-{}", snapshot.remote_id())).into()
    }
}

fn predict_edits_request_trigger_from_editor_trigger(
    trigger: EditPredictionRequestTrigger,
) -> PredictEditsRequestTrigger {
    match trigger {
        EditPredictionRequestTrigger::DiagnosticNavigation => {
            PredictEditsRequestTrigger::DiagnosticNavigation
        }
        EditPredictionRequestTrigger::Explicit => PredictEditsRequestTrigger::Explicit,
        EditPredictionRequestTrigger::BufferEdit => PredictEditsRequestTrigger::BufferEdit,
        EditPredictionRequestTrigger::LSPCompletionAccepted => {
            PredictEditsRequestTrigger::LSPCompletionAccepted
        }
        EditPredictionRequestTrigger::PredictionAccepted => {
            PredictEditsRequestTrigger::PredictionAccepted
        }
        EditPredictionRequestTrigger::PredictionPartiallyAccepted => {
            PredictEditsRequestTrigger::PredictionPartiallyAccepted
        }
        EditPredictionRequestTrigger::Other => PredictEditsRequestTrigger::Other,
    }
}
