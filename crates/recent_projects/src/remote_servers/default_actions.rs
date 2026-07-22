use super::*;

impl RemoteServerProjects {
    fn render_default(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .min_h(rems(20.))
            .size_full()
            .child(self.default_picker.clone())
            .into_any_element()
    }

    fn create_host_from_ssh_config(
        &mut self,
        ssh_config_host: &SharedString,
        cx: &mut Context<'_, Self>,
    ) -> SshServerIndex {
        let new_ix = RemoteSettings::get_global(cx).ssh_connections().count();

        self.add_ssh_server(
            SshConnectionOptions {
                host: ssh_config_host.to_string().into(),
                ..SshConnectionOptions::default()
            },
            cx,
        );
        self.mode = Mode::default_mode(&self.ssh_config_servers, cx);
        SshServerIndex(new_ix)
    }
}
