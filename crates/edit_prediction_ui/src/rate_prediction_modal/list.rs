use super::*;

impl RatePredictionsModal {
    pub(super) fn render_shown_completions(
        &self,
        cx: &Context<Self>,
    ) -> impl Iterator<Item = ListItem> {
        self.ep_store
            .read(cx)
            .rateable_predictions()
            .cloned()
            .enumerate()
            .map(|(index, completion)| {
                let selected = self
                    .active_prediction
                    .as_ref()
                    .is_some_and(|selected| selected.prediction.id == completion.id);
                let rated = self.ep_store.read(cx).is_prediction_rated(&completion.id);

                let (icon_name, icon_color, tooltip_text) =
                    match (rated, completion.edits.is_empty()) {
                        (true, _) => (IconName::Check, Color::Success, "Rated Prediction"),
                        (false, true) => (IconName::File, Color::Muted, "No Edits Produced"),
                        (false, false) => (IconName::FileDiff, Color::Accent, "Edits Available"),
                    };
                let (trigger_icon, trigger_tooltip) = match completion.trigger {
                    PredictEditsRequestTrigger::Testing => (IconName::Debug, "Testing"),
                    PredictEditsRequestTrigger::Diagnostics => {
                        (IconName::ToolDiagnostics, "Diagnostics")
                    }
                    PredictEditsRequestTrigger::DiagnosticNavigation => {
                        (IconName::ArrowRight, "Diagnostic Navigation")
                    }
                    PredictEditsRequestTrigger::Cli => (IconName::Terminal, "CLI"),
                    PredictEditsRequestTrigger::Explicit => (IconName::Person, "Explicit"),
                    PredictEditsRequestTrigger::BufferEdit => (IconName::Pencil, "Buffer Edit"),
                    PredictEditsRequestTrigger::LSPCompletionAccepted => {
                        (IconName::Code, "LSP Completion Accepted")
                    }
                    PredictEditsRequestTrigger::PredictionAccepted => {
                        (IconName::MavPredict, "Prediction Accepted")
                    }
                    PredictEditsRequestTrigger::PredictionPartiallyAccepted => {
                        (IconName::CheckDouble, "Prediction Partially Accepted")
                    }
                    PredictEditsRequestTrigger::Other => (IconName::CircleHelp, "Other"),
                };

                let file = completion.buffer.read(cx).file();
                let file_name = file
                    .as_ref()
                    .map_or(SharedString::new_static("untitled"), |file| {
                        file.file_name(cx).to_string().into()
                    });
                let file_path = file.map(|file| file.path().as_unix_str().to_string());

                ListItem::new(completion.id.clone())
                    .inset(true)
                    .spacing(ListItemSpacing::Sparse)
                    .focused(index == self.selected_index)
                    .toggle_state(selected)
                    .child(
                        h_flex()
                            .id("completion-content")
                            .gap_3()
                            .child(Icon::new(icon_name).color(icon_color).size(IconSize::Small))
                            .child(
                                Icon::new(trigger_icon)
                                    .color(Color::Muted)
                                    .size(IconSize::XSmall),
                            )
                            .child(
                                v_flex().child(
                                    h_flex()
                                        .gap_1()
                                        .child(Label::new(file_name).size(LabelSize::Small))
                                        .when_some(file_path, |this, p| {
                                            this.child(
                                                Label::new(p)
                                                    .size(LabelSize::Small)
                                                    .color(Color::Muted),
                                            )
                                        }),
                                ),
                            ),
                    )
                    .tooltip(Tooltip::text(format!(
                        "{tooltip_text} • Trigger: {trigger_tooltip}"
                    )))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_completion(Some(completion.clone()), true, window, cx);
                    }))
            })
    }
}
