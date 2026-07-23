use super::*;

impl Editor {
    pub(crate) fn on_buffer_changed(&mut self, _: Entity<MultiBuffer>, cx: &mut Context<Self>) {
        cx.notify();
    }

    pub(crate) fn on_debug_session_event(
        &mut self,
        _session: Entity<Session>,
        event: &SessionEvent,
        cx: &mut Context<Self>,
    ) {
        if let SessionEvent::InvalidateInlineValue = event {
            self.refresh_inline_values(cx);
        }
    }

    pub fn refresh_inline_values(&mut self, cx: &mut Context<Self>) {
        let Some(semantics) = self.semantics_provider.clone() else {
            return;
        };

        if !self.inline_value_cache.enabled {
            let inlays = std::mem::take(&mut self.inline_value_cache.inlays);
            self.splice_inlays(&inlays, Vec::new(), cx);
            return;
        }

        let current_execution_position = self
            .highlighted_rows
            .get(&TypeId::of::<ActiveDebugLine>())
            .and_then(|lines| lines.last().map(|line| line.range.end));

        self.inline_value_cache.refresh_task = cx.spawn(async move |editor, cx| {
            let inline_values = editor
                .update(cx, |editor, cx| {
                    let Some(current_execution_position) = current_execution_position else {
                        return Some(Task::ready(Ok(Vec::new())));
                    };

                    let (buffer, buffer_anchor) =
                        editor.buffer.read_with(cx, |multibuffer, cx| {
                            let multibuffer_snapshot = multibuffer.snapshot(cx);
                            let (buffer_anchor, _) = multibuffer_snapshot
                                .anchor_to_buffer_anchor(current_execution_position)?;
                            let buffer = multibuffer.buffer(buffer_anchor.buffer_id)?;
                            Some((buffer, buffer_anchor))
                        })?;

                    let range = buffer.read(cx).anchor_before(0)..buffer_anchor;

                    semantics.inline_values(buffer, range, cx)
                })
                .ok()
                .flatten()?
                .await
                .context("refreshing debugger inlays")
                .log_err()?;

            let mut buffer_inline_values: HashMap<BufferId, Vec<InlayHint>> = HashMap::default();

            for (buffer_id, inline_value) in inline_values
                .into_iter()
                .map(|hint| (hint.position.buffer_id, hint))
            {
                buffer_inline_values
                    .entry(buffer_id)
                    .or_default()
                    .push(inline_value);
            }

            editor
                .update(cx, |editor, cx| {
                    let snapshot = editor.buffer.read(cx).snapshot(cx);
                    let mut new_inlays = Vec::default();

                    for (_buffer_id, inline_values) in buffer_inline_values {
                        for hint in inline_values {
                            let Some(anchor) = snapshot.anchor_in_excerpt(hint.position) else {
                                continue;
                            };
                            let inlay = Inlay::debugger(
                                post_inc(&mut editor.next_inlay_id),
                                anchor,
                                hint.text(),
                            );
                            if !inlay.text().chars().contains(&'\n') {
                                new_inlays.push(inlay);
                            }
                        }
                    }

                    let mut inlay_ids = new_inlays.iter().map(|inlay| inlay.id).collect();
                    std::mem::swap(&mut editor.inline_value_cache.inlays, &mut inlay_ids);

                    editor.splice_inlays(&inlay_ids, new_inlays, cx);
                })
                .ok()?;
            Some(())
        });
    }

    pub(crate) fn on_display_map_changed(
        &mut self,
        _: Entity<DisplayMap>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.notify();
    }
}
