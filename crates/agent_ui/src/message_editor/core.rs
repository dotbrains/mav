use super::*;

impl MessageEditor {
    pub fn set_session_capabilities(
        &mut self,
        session_capabilities: SharedSessionCapabilities,
        _cx: &mut Context<Self>,
    ) {
        self.session_capabilities = session_capabilities;
    }

    fn command_hint(&self, snapshot: &MultiBufferSnapshot) -> Option<Inlay> {
        let session_capabilities = self.session_capabilities.read();
        let available_commands = session_capabilities.available_commands();
        if available_commands.is_empty() {
            return None;
        }

        let parsed_command = SlashCommandCompletion::try_parse(&snapshot.text(), 0)?;
        if parsed_command.argument.is_some() {
            return None;
        }

        let command_name = parsed_command.command?;
        let available_command = available_commands
            .iter()
            .find(|available_command| available_command.name == command_name)?;

        let acp::AvailableCommandInput::Unstructured(acp::UnstructuredCommandInput {
            mut hint,
            ..
        }) = available_command.input.clone()?
        else {
            return None;
        };

        let mut hint_pos = MultiBufferOffset(parsed_command.source_range.end) + 1usize;
        if hint_pos > snapshot.len() {
            hint_pos = snapshot.len();
            hint.insert(0, ' ');
        }

        let hint_pos = snapshot.anchor_after(hint_pos);

        Some(Inlay::hint(
            COMMAND_HINT_INLAY_ID,
            hint_pos,
            &InlayHint {
                position: snapshot.anchor_to_buffer_anchor(hint_pos)?.0,
                label: InlayHintLabel::String(hint),
                kind: Some(InlayHintKind::Parameter),
                padding_left: false,
                padding_right: false,
                tooltip: None,
                resolve_state: project::ResolveState::Resolved,
            },
        ))
    }

    pub fn insert_thread_summary(
        &mut self,
        session_id: acp::SessionId,
        title: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_store.is_none() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let thread_title = title
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| SharedString::new_static(DEFAULT_THREAD_TITLE));
        let uri = MentionUri::Thread {
            id: session_id,
            name: thread_title.to_string(),
        };
        let content = format!("{}\n", uri.as_link());

        let content_len = content.len() - 1;

        let start = self.editor.update(cx, |editor, cx| {
            editor.set_text(content, window, cx);
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            snapshot
                .anchor_to_buffer_anchor(snapshot.anchor_before(Point::zero()))
                .unwrap()
                .0
        });

        let supports_images = self.session_capabilities.read().supports_images();

        self.mention_set
            .update(cx, |mention_set, cx| {
                mention_set.confirm_mention_completion(
                    thread_title,
                    start,
                    content_len,
                    uri,
                    supports_images,
                    self.editor.clone(),
                    &workspace,
                    window,
                    cx,
                )
            })
            .detach();
    }

    pub(crate) fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.editor.read(cx).text(cx).trim().is_empty()
    }

    pub fn is_completions_menu_visible(&self, cx: &App) -> bool {
        self.editor
            .read(cx)
            .context_menu()
            .borrow()
            .as_ref()
            .is_some_and(|menu| matches!(menu, CodeContextMenu::Completions(_)) && menu.visible())
    }

    #[cfg(test)]
    pub fn mention_set(&self) -> &Entity<MentionSet> {
        &self.mention_set
    }
}
