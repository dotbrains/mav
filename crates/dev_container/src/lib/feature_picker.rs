use super::*;

pub(super) struct FeaturePickerDelegate {
    selected_index: usize,
    placeholder_text: String,
    stateful_modal: WeakEntity<DevContainerModal>,
    candidate_features: Vec<FeatureEntry>,
    template_entry: TemplateEntry,
    matching_indices: Vec<usize>,
    on_confirm: Box<
        dyn FnMut(
            TemplateEntry,
            &mut DevContainerModal,
            &mut Window,
            &mut Context<DevContainerModal>,
        ),
    >,
}

impl FeaturePickerDelegate {
    pub(super) fn new(
        placeholder_text: String,
        stateful_modal: WeakEntity<DevContainerModal>,
        candidate_features: Vec<FeatureEntry>,
        template_entry: TemplateEntry,
        on_confirm: Box<
            dyn FnMut(
                TemplateEntry,
                &mut DevContainerModal,
                &mut Window,
                &mut Context<DevContainerModal>,
            ),
        >,
    ) -> Self {
        Self {
            selected_index: 0,
            placeholder_text,
            stateful_modal,
            candidate_features,
            template_entry,
            matching_indices: Vec::new(),
            on_confirm,
        }
    }
}

impl PickerDelegate for FeaturePickerDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "dev container feature picker"
    }

    fn match_count(&self) -> usize {
        self.matching_indices.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        self.placeholder_text.clone().into()
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.matching_indices = self
            .candidate_features
            .iter()
            .enumerate()
            .filter(|(_, feature_entry)| {
                feature_entry
                    .feature
                    .id
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                    || feature_entry
                        .feature
                        .name
                        .to_lowercase()
                        .contains(&query.to_lowercase())
            })
            .map(|(ix, _)| ix)
            .collect();
        self.selected_index = std::cmp::min(
            self.selected_index,
            self.matching_indices.len().saturating_sub(1),
        );
        Task::ready(())
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if secondary {
            self.stateful_modal
                .update(cx, |modal, cx| {
                    (self.on_confirm)(self.template_entry.clone(), modal, window, cx)
                })
                .ok();
        } else {
            if self.matching_indices.is_empty() {
                return;
            }
            let Some(current) = self
                .matching_indices
                .get(self.selected_index)
                .and_then(|ix| self.candidate_features.get_mut(*ix))
            else {
                log::error!("Selected index not in range of matches");
                return;
            };
            current.toggle_state = match current.toggle_state {
                ToggleState::Selected => {
                    self.template_entry
                        .features_selected
                        .remove(&current.feature);
                    ToggleState::Unselected
                }
                _ => {
                    self.template_entry
                        .features_selected
                        .insert(current.feature.clone());
                    ToggleState::Selected
                }
            };
        }
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.stateful_modal
            .update(cx, |modal, cx| {
                modal.dismiss(&menu::Cancel, window, cx);
            })
            .ok();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let feature_entry = self.candidate_features[self.matching_indices[ix]].clone();

        Some(
            ListItem::new("li-what")
                .inset(true)
                .toggle_state(selected)
                .start_slot(Switch::new(
                    feature_entry.feature.id.clone(),
                    feature_entry.toggle_state,
                ))
                .child(Label::new(feature_entry.feature.name))
                .into_any_element(),
        )
    }

    fn render_footer(
        &self,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .gap_1()
                .justify_start()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(
                    Button::new("run-action", "Select Feature")
                        .key_binding(
                            KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|kb| kb.size(rems_from_px(12.0_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .child(
                    Button::new("run-action-secondary", "Confirm Selections")
                        .key_binding(
                            KeyBinding::for_action(&menu::SecondaryConfirm, cx)
                                .map(|kb| kb.size(rems_from_px(12.0_f32))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                        }),
                )
                .into_any_element(),
        )
    }
}
