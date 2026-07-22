use super::*;

impl ModalView for RemoteServerProjects {
    fn on_before_dismiss(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> DismissDecision {
        DismissDecision::Dismiss(self.allow_dismissal)
    }
}

impl Focusable for RemoteServerProjects {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.mode {
            Mode::Default => self.default_picker.focus_handle(cx),
            Mode::ProjectPicker(picker) => picker.focus_handle(cx),
            _ => self.focus_handle.clone(),
        }
    }
}

impl EventEmitter<DismissEvent> for RemoteServerProjects {}

impl Render for RemoteServerProjects {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .elevation_3(cx)
            .w(rems(34.))
            .key_context("RemoteServerModal")
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.focus_handle(cx).focus(window, cx);
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if matches!(this.mode, Mode::Default) {
                    cx.emit(DismissEvent)
                }
            }))
            .child(match &self.mode {
                Mode::Default => self.render_default(window, cx).into_any_element(),
                Mode::ViewServerOptions(state) => self
                    .render_view_options(state.clone(), window, cx)
                    .into_any_element(),
                Mode::ProjectPicker(element) => element.clone().into_any_element(),
                Mode::CreateRemoteServer(state) => self
                    .render_create_remote_server(state, window, cx)
                    .into_any_element(),
                Mode::CreateRemoteDevContainer(state) => self
                    .render_create_dev_container(state, window, cx)
                    .into_any_element(),
                Mode::EditNickname(state) => self
                    .render_edit_nickname(state, window, cx)
                    .into_any_element(),
                #[cfg(target_os = "windows")]
                Mode::AddWslDistro(state) => self
                    .render_add_wsl_distro(state, window, cx)
                    .into_any_element(),
            })
    }
}
