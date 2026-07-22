use super::*;

pub(super) struct DevContainerPickerDelegate {
    selected_index: usize,
    candidates: Vec<DevContainerConfig>,
    matching_candidates: Vec<DevContainerConfig>,
    parent_modal: WeakEntity<RemoteServerProjects>,
}
impl DevContainerPickerDelegate {
    pub(super) fn new(
        candidates: Vec<DevContainerConfig>,
        parent_modal: WeakEntity<RemoteServerProjects>,
    ) -> Self {
        Self {
            selected_index: 0,
            matching_candidates: candidates.clone(),
            candidates,
            parent_modal,
        }
    }
}

impl PickerDelegate for DevContainerPickerDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "remote dev container picker"
    }

    fn match_count(&self) -> usize {
        self.matching_candidates.len()
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
        "Select Dev Container Configuration".into()
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query_lower = query.to_lowercase();
        self.matching_candidates = self
            .candidates
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&query_lower)
                    || c.config_path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query_lower)
            })
            .cloned()
            .collect();

        self.selected_index = std::cmp::min(
            self.selected_index,
            self.matching_candidates.len().saturating_sub(1),
        );

        Task::ready(())
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let selected_config = self.matching_candidates.get(self.selected_index).cloned();
        self.parent_modal
            .update(cx, move |modal, cx| {
                if secondary {
                    modal.edit_in_dev_container_json(selected_config.clone(), window, cx);
                } else if let Some((app_state, context)) = modal
                    .workspace
                    .read_with(cx, |workspace, cx| {
                        let app_state = workspace.app_state().clone();
                        let context = DevContainerContext::from_workspace(workspace, cx)?;
                        Some((app_state, context))
                    })
                    .ok()
                    .flatten()
                {
                    modal.open_dev_container(selected_config, app_state, context, window, cx);
                    modal.view_in_progress_dev_container(window, cx);
                } else {
                    log::error!("No active project directory for Dev Container");
                }
            })
            .ok();
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.parent_modal
            .update(cx, |modal, cx| {
                modal.cancel(&menu::Cancel, window, cx);
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
        let candidate = self.matching_candidates.get(ix)?;
        let config_path = candidate.config_path.display().to_string();
        Some(
            ListItem::new(SharedString::from(format!("li-devcontainer-config-{}", ix)))
                .inset(true)
                .spacing(ui::ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(Icon::new(IconName::FileToml).color(Color::Muted))
                .child(
                    v_flex().child(Label::new(candidate.name.clone())).child(
                        Label::new(config_path)
                            .size(ui::LabelSize::Small)
                            .color(Color::Muted),
                    ),
                )
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
                    Button::new("run-action", "Start Dev Container")
                        .key_binding(
                            KeyBinding::for_action(&menu::Confirm, cx)
                                .map(|kb| kb.size(rems_from_px(12.))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                        }),
                )
                .child(
                    Button::new("run-action-secondary", "Open devcontainer.json")
                        .key_binding(
                            KeyBinding::for_action(&menu::SecondaryConfirm, cx)
                                .map(|kb| kb.size(rems_from_px(12.))),
                        )
                        .on_click(|_, window, cx| {
                            window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                        }),
                )
                .into_any_element(),
        )
    }
}
