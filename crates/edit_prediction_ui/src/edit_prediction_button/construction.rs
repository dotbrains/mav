use super::*;

impl EditPredictionButton {
    pub fn new(
        fs: Arc<dyn Fs>,
        user_store: Entity<UserStore>,
        popover_menu_handle: PopoverMenuHandle<ContextMenu>,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Self {
        let copilot = EditPredictionStore::try_global(cx).and_then(|store| {
            store.update(cx, |this, cx| this.start_copilot_for_project(&project, cx))
        });
        if let Some(copilot) = copilot {
            cx.observe(&copilot, |_, _, cx| cx.notify()).detach()
        }

        cx.observe_global::<SettingsStore>(move |_, cx| cx.notify())
            .detach();

        cx.observe_global::<EditPredictionStore>(move |_, cx| cx.notify())
            .detach();

        edit_prediction::ollama::ensure_authenticated(cx);
        let mercury_api_token_task = edit_prediction::mercury::load_mercury_api_token(cx);
        let open_ai_compatible_api_token_task =
            edit_prediction::open_ai_compatible::load_open_ai_compatible_api_token(cx);

        cx.spawn(async move |this, cx| {
            _ = futures::join!(mercury_api_token_task, open_ai_compatible_api_token_task);
            this.update(cx, |_, cx| {
                cx.notify();
            })
            .ok();
        })
        .detach();

        CodestralEditPredictionDelegate::ensure_api_key_loaded(cx);

        Self {
            editor_subscription: None,
            editor_enabled: None,
            editor_show_predictions: true,
            editor_focus_handle: None,
            language: None,
            file: None,
            edit_prediction_provider: None,
            user_store,
            popover_menu_handle,
            project: project.downgrade(),
            fs,
        }
    }
}
