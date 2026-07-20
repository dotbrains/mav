use super::*;

impl Editor {
    pub(crate) fn restart_language_server(
        &mut self,
        _: &RestartLanguageServer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.project.clone() {
            self.buffer.update(cx, |multi_buffer, cx| {
                project.update(cx, |project, cx| {
                    project.restart_language_servers_for_buffers(
                        multi_buffer.all_buffers().into_iter().collect(),
                        HashSet::default(),
                        true,
                        cx,
                    );
                });
            })
        }
    }

    pub(crate) fn stop_language_server(
        &mut self,
        _: &StopLanguageServer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.project.clone() {
            self.buffer.update(cx, |multi_buffer, cx| {
                project.update(cx, |project, cx| {
                    project.stop_language_servers_for_buffers(
                        multi_buffer.all_buffers().into_iter().collect(),
                        HashSet::default(),
                        cx,
                    );
                });
            });
        }
    }

    pub(crate) fn cancel_language_server_work(
        workspace: &mut Workspace,
        _: &actions::CancelLanguageServerWork,
        _: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let project = workspace.project();
        let buffers = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
            .map_or(HashSet::default(), |editor| {
                editor.read(cx).buffer.read(cx).all_buffers()
            });
        project.update(cx, |project, cx| {
            project.cancel_language_server_work_for_buffers(buffers, cx);
        });
    }

    pub(crate) fn lsp_data_enabled(&self) -> bool {
        self.enable_lsp_data && self.mode().is_full()
    }

    pub(crate) fn update_lsp_data(
        &mut self,
        for_buffer: Option<BufferId>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.lsp_data_enabled() {
            return;
        }

        if let Some(buffer_id) = for_buffer {
            self.pull_diagnostics(buffer_id, window, cx);
        }
        self.refresh_semantic_tokens(for_buffer, None, cx);
        self.refresh_document_colors(for_buffer, window, cx);
        self.refresh_document_links(for_buffer, cx);
        self.refresh_folding_ranges(for_buffer, window, cx);
        self.refresh_code_lenses(for_buffer, window, cx);
        self.refresh_document_symbols(for_buffer, cx);
    }

    pub(crate) fn register_visible_buffers(&mut self, cx: &mut Context<Self>) {
        if !self.lsp_data_enabled() {
            return;
        }
        let visible_buffers: Vec<_> = self
            .visible_buffers(cx)
            .into_iter()
            .filter(|buffer| self.is_lsp_relevant(buffer.read(cx).file(), cx))
            .collect();
        for visible_buffer in visible_buffers {
            self.register_buffer(visible_buffer.read(cx).remote_id(), cx);
        }
    }

    pub(crate) fn register_buffer(&mut self, buffer_id: BufferId, cx: &mut Context<Self>) {
        if !self.lsp_data_enabled() {
            return;
        }

        if !self.registered_buffers.contains_key(&buffer_id)
            && let Some(project) = self.project.as_ref()
        {
            if let Some(buffer) = self.buffer.read(cx).buffer(buffer_id) {
                project.update(cx, |project, cx| {
                    self.registered_buffers.insert(
                        buffer_id,
                        project.register_buffer_with_language_servers(&buffer, cx),
                    );
                });
            } else {
                self.registered_buffers.remove(&buffer_id);
            }
        }
    }
}
