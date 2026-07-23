use super::*;

impl RatePredictionsModal {
    pub(super) fn dismiss(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    pub(super) fn select_next(
        &mut self,
        _: &menu::SelectNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_index += 1;
        self.selected_index = usize::min(
            self.selected_index,
            self.ep_store.read(cx).rateable_predictions().count(),
        );
        cx.notify();
    }

    pub(super) fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_index = self.selected_index.saturating_sub(1);
        cx.notify();
    }

    pub(super) fn select_next_edit(
        &mut self,
        _: &NextEdit,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_index = self
            .ep_store
            .read(cx)
            .rateable_predictions()
            .skip(self.selected_index)
            .enumerate()
            .skip(1) // Skip straight to the next item
            .find(|(_, completion)| !completion.edits.is_empty())
            .map(|(ix, _)| ix + self.selected_index);

        if let Some(next_index) = next_index {
            self.selected_index = next_index;
            cx.notify();
        }
    }

    pub(super) fn select_prev_edit(
        &mut self,
        _: &PreviousEdit,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ep_store = self.ep_store.read(cx);
        let completions_len = ep_store.rateable_predictions_count();

        let prev_index = self
            .ep_store
            .read(cx)
            .rateable_predictions()
            .rev()
            .skip((completions_len - 1) - self.selected_index)
            .enumerate()
            .skip(1) // Skip straight to the previous item
            .find(|(_, completion)| !completion.edits.is_empty())
            .map(|(ix, _)| self.selected_index - ix);

        if let Some(prev_index) = prev_index {
            self.selected_index = prev_index;
            cx.notify();
        }
        cx.notify();
    }

    pub(super) fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_index = 0;
        cx.notify();
    }

    pub(super) fn select_last(
        &mut self,
        _: &menu::SelectLast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_index = self.ep_store.read(cx).rateable_predictions_count() - 1;
        cx.notify();
    }

    pub fn thumbs_up_active(
        &mut self,
        _: &ThumbsUpActivePrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ep_store.update(cx, |ep_store, cx| {
            if let Some(active) = &self.active_prediction {
                ep_store.rate_prediction(
                    &active.prediction,
                    EditPredictionRating::Positive,
                    active.feedback_editor.read(cx).text(cx),
                    self.expected_patch_for_active(cx),
                    cx,
                );
            }
        });

        let current_completion = self
            .active_prediction
            .as_ref()
            .map(|completion| completion.prediction.clone());
        self.select_completion(current_completion, false, window, cx);
        self.select_next_edit(&Default::default(), window, cx);
        self.confirm(&Default::default(), window, cx);

        cx.notify();
    }

    pub fn thumbs_down_active(
        &mut self,
        _: &ThumbsDownActivePrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(active) = &self.active_prediction {
            if active.feedback_editor.read(cx).text(cx).is_empty() {
                return;
            }

            self.ep_store.update(cx, |ep_store, cx| {
                ep_store.rate_prediction(
                    &active.prediction,
                    EditPredictionRating::Negative,
                    active.feedback_editor.read(cx).text(cx),
                    self.expected_patch_for_active(cx),
                    cx,
                );
            });
        }

        let current_completion = self
            .active_prediction
            .as_ref()
            .map(|completion| completion.prediction.clone());
        self.select_completion(current_completion, false, window, cx);
        self.select_next_edit(&Default::default(), window, cx);
        self.confirm(&Default::default(), window, cx);

        cx.notify();
    }

    pub(super) fn focus_completions(
        &mut self,
        _: &FocusPredictions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.focus_self(window);
        cx.notify();
    }

    pub(super) fn preview_completion(
        &mut self,
        _: &PreviewPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let completion = self
            .ep_store
            .read(cx)
            .rateable_predictions()
            .skip(self.selected_index)
            .take(1)
            .next()
            .cloned();

        self.select_completion(completion, false, window, cx);
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let completion = self
            .ep_store
            .read(cx)
            .rateable_predictions()
            .skip(self.selected_index)
            .take(1)
            .next()
            .cloned();

        self.select_completion(completion, true, window, cx);
    }
}
