use buffer_diff::BufferDiff;
use cloud_llm_client::PredictEditsRequestTrigger;
use edit_prediction::{
    EditPrediction, EditPredictionInputs, EditPredictionRating, EditPredictionStore,
};
use editor::{Editor, Inlay, MultiBuffer};
use feature_flags::{FeatureFlag, PresenceFlag, register_feature_flag};
use gpui::{
    App, BorderStyle, DismissEvent, EdgesRefinement, Entity, EventEmitter, FocusHandle, Focusable,
    Length, StyleRefinement, Task, TextStyleRefinement, Window, actions, prelude::*,
};
use language::{
    Bias, Buffer, BufferSnapshot, CodeLabel, LanguageRegistry, Point, ToOffset, ToPoint,
    language_settings::{self, InlayHintKind},
};
use markdown::{Markdown, MarkdownStyle};
use project::{
    Completion, CompletionDisplayOptions, CompletionResponse, CompletionSource, InlayHint,
    InlayHintLabel, InlayId, ResolveState,
};
use settings::Settings as _;
use std::rc::Rc;
use std::{fmt::Write, ops::Range, path::Path, sync::Arc};
use theme_settings::ThemeSettings;
use ui::{
    ContextMenu, DropdownMenu, KeyBinding, List, ListItem, ListItemSpacing, PopoverMenuHandle,
    Tooltip, prelude::*,
};
use workspace::{ModalView, Workspace};
use zeta_prompt::{ContextSource, FilePosition, RelatedExcerpt, RelatedFile, Zeta3PromptInput};

mod actions;
mod diff;
mod feedback_completion;
mod formatted_inputs;
mod list;
mod render;

use feedback_completion::FeedbackCompletionProvider;
actions!(
    zeta,
    [
        /// Rates the active completion with a thumbs up.
        ThumbsUpActivePrediction,
        /// Rates the active completion with a thumbs down.
        ThumbsDownActivePrediction,
        /// Navigates to the next edit in the completion history.
        NextEdit,
        /// Navigates to the previous edit in the completion history.
        PreviousEdit,
        /// Focuses on the completions list.
        FocusPredictions,
        /// Previews the selected completion.
        PreviewPrediction,
    ]
);

pub struct PredictEditsRatePredictionsFeatureFlag;

impl FeatureFlag for PredictEditsRatePredictionsFeatureFlag {
    const NAME: &'static str = "predict-edits-rate-completions";
    type Value = PresenceFlag;
}
register_feature_flag!(PredictEditsRatePredictionsFeatureFlag);

pub struct RatePredictionsModal {
    ep_store: Entity<EditPredictionStore>,
    language_registry: Arc<LanguageRegistry>,
    active_prediction: Option<ActivePrediction>,
    selected_index: usize,
    diff_editor: Entity<Editor>,
    focus_handle: FocusHandle,
    _subscription: gpui::Subscription,
    current_view: RatePredictionView,
    failure_mode_menu_handle: PopoverMenuHandle<ContextMenu>,
}

struct ActivePrediction {
    prediction: EditPrediction,
    feedback_editor: Entity<Editor>,
    expected_buffer: Entity<Buffer>,
    expected_editor: Entity<Editor>,
    _expected_buffer_subscription: gpui::Subscription,
    formatted_inputs: Entity<Markdown>,
    _predicted_diff_task: Task<()>,
    expected_diff_task: Task<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
enum RatePredictionView {
    SuggestedEdits,
    RawInput,
}

impl RatePredictionView {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SuggestedEdits => "Suggested Edits",
            Self::RawInput => "Recorded Events & Input",
        }
    }
}

impl RatePredictionsModal {
    pub fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        if let Some(ep_store) = EditPredictionStore::try_global(cx) {
            let language_registry = workspace.app_state().languages.clone();
            workspace.toggle_modal(window, cx, |window, cx| {
                RatePredictionsModal::new(ep_store, language_registry, window, cx)
            });

            telemetry::event!("Rate Prediction Modal Open", source = "Edit Prediction");
        }
    }

    pub fn new(
        ep_store: Entity<EditPredictionStore>,
        language_registry: Arc<LanguageRegistry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&ep_store, |_, _, cx| cx.notify());

        Self {
            ep_store,
            language_registry,
            selected_index: 0,
            focus_handle: cx.focus_handle(),
            active_prediction: None,
            _subscription: subscription,
            diff_editor: cx.new(|cx| {
                let multibuffer = cx.new(|_| MultiBuffer::new(language::Capability::ReadOnly));
                let mut editor = Editor::for_multibuffer(multibuffer, None, window, cx);
                editor.disable_inline_diagnostics();
                editor.set_expand_all_diff_hunks(cx);
                editor.set_show_git_diff_gutter(false, cx);
                editor
            }),
            current_view: RatePredictionView::SuggestedEdits,
            failure_mode_menu_handle: PopoverMenuHandle::default(),
        }
    }
}

impl EventEmitter<DismissEvent> for RatePredictionsModal {}

impl Focusable for RatePredictionsModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for RatePredictionsModal {}
