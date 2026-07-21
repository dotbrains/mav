use super::*;

impl MultiBuffer {
    // If point is at the end of the buffer, the last excerpt is returned
    pub fn point_to_buffer_offset<T: ToOffset>(
        &self,
        point: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, BufferOffset)> {
        let snapshot = self.read(cx);
        let (buffer, offset) = snapshot.point_to_buffer_offset(point)?;
        Some((
            self.buffers.get(&buffer.remote_id())?.buffer.clone(),
            offset,
        ))
    }

    // If point is at the end of the buffer, the last excerpt is returned
    pub fn point_to_buffer_point<T: ToPoint>(
        &self,
        point: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, Point)> {
        let snapshot = self.read(cx);
        let (buffer, point) = snapshot.point_to_buffer_point(point.to_point(&snapshot))?;
        Some((self.buffers.get(&buffer.remote_id())?.buffer.clone(), point))
    }

    pub fn buffer_point_to_anchor(
        &self,
        // todo(lw): We shouldn't need this?
        buffer: &Entity<Buffer>,
        point: Point,
        cx: &App,
    ) -> Option<Anchor> {
        let mut found = None;
        let buffer_snapshot = buffer.read(cx).snapshot();
        let text_anchor = buffer_snapshot.anchor_after(&point);
        let snapshot = self.snapshot(cx);
        let path_key_index = snapshot.path_key_index_for_buffer(buffer_snapshot.remote_id())?;
        for excerpt in snapshot.excerpts_for_buffer(buffer_snapshot.remote_id()) {
            if excerpt
                .context
                .start
                .cmp(&text_anchor, &buffer_snapshot)
                .is_gt()
            {
                found = Some(Anchor::in_buffer(path_key_index, excerpt.context.start));
                break;
            } else if excerpt
                .context
                .end
                .cmp(&text_anchor, &buffer_snapshot)
                .is_ge()
            {
                found = Some(Anchor::in_buffer(path_key_index, text_anchor));
                break;
            }
            found = Some(Anchor::in_buffer(path_key_index, excerpt.context.end));
        }

        found
    }

    pub fn wait_for_anchors<'a, Anchors: 'a + Iterator<Item = Anchor>>(
        &self,
        anchors: Anchors,
        cx: &mut Context<Self>,
    ) -> impl 'static + Future<Output = Result<()>> + use<Anchors> {
        let mut error = None;
        let mut futures = Vec::new();
        for anchor in anchors {
            if let Some(excerpt_anchor) = anchor.excerpt_anchor() {
                if let Some(buffer) = self.buffers.get(&excerpt_anchor.text_anchor.buffer_id) {
                    buffer.buffer.update(cx, |buffer, _| {
                        futures.push(buffer.wait_for_anchors([excerpt_anchor.text_anchor()]))
                    });
                } else {
                    error = Some(anyhow!(
                        "buffer {:?} is not part of this multi-buffer",
                        excerpt_anchor.text_anchor.buffer_id
                    ));
                    break;
                }
            }
        }
        async move {
            if let Some(error) = error {
                Err(error)?;
            }
            for future in futures {
                future.await?;
            }
            Ok(())
        }
    }

    pub fn text_anchor_for_position<T: ToOffset>(
        &self,
        position: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, text::Anchor)> {
        let snapshot = self.read(cx);
        let anchor = snapshot.anchor_before(position).excerpt_anchor()?;
        let buffer = self
            .buffers
            .get(&anchor.text_anchor.buffer_id)?
            .buffer
            .clone();
        Some((buffer, anchor.text_anchor()))
    }

    pub(super) fn on_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &language::BufferEvent,
        cx: &mut Context<Self>,
    ) {
        use language::BufferEvent;
        let buffer_id = buffer.read(cx).remote_id();
        cx.emit(match event {
            &BufferEvent::Edited { source } => Event::Edited {
                edited_buffer: Some(buffer),
                source,
            },
            BufferEvent::DirtyChanged => Event::DirtyChanged,
            BufferEvent::Saved => Event::Saved,
            BufferEvent::FileHandleChanged => Event::FileHandleChanged,
            BufferEvent::Reloaded => Event::Reloaded,
            BufferEvent::LanguageChanged(has_language) => {
                Event::LanguageChanged(buffer_id, *has_language)
            }
            BufferEvent::Reparsed => Event::Reparsed(buffer_id),
            BufferEvent::DiagnosticsUpdated => Event::DiagnosticsUpdated,
            BufferEvent::CapabilityChanged => {
                self.capability = buffer.read(cx).capability();
                return;
            }
            BufferEvent::Operation { .. } | BufferEvent::ReloadNeeded => return,
        });
    }

    pub(super) fn buffer_diff_changed(
        &mut self,
        diff: Entity<BufferDiff>,
        range: Option<Range<text::Anchor>>,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.buffer(diff.read(cx).buffer_id) else {
            return;
        };
        let snapshot = self.sync_mut(cx);

        let diff = diff.read(cx);
        let buffer_id = diff.buffer_id;

        let Some(path) = snapshot.path_for_buffer(buffer_id).cloned() else {
            return;
        };
        let new_diff = DiffStateSnapshot {
            buffer_id,
            diff: diff.snapshot(cx),
            main_buffer: None,
        };
        let snapshot = self.snapshot.get_mut();
        let base_text_changed = find_diff_state(&snapshot.diffs, buffer_id)
            .is_none_or(|old_diff| !new_diff.base_texts_definitely_eq(old_diff));
        snapshot.diffs.insert_or_replace(new_diff, ());

        let buffer = buffer.read(cx);
        let Some(range) = range else {
            return;
        };
        let diff_change_range = range.to_offset(buffer);

        let excerpt_edits = snapshot.excerpt_edits_for_diff_change(&path, diff_change_range);
        let edits = Self::sync_diff_transforms(
            snapshot,
            excerpt_edits,
            DiffChangeKind::DiffUpdated {
                base_changed: base_text_changed,
            },
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    pub(super) fn inverted_buffer_diff_changed(
        &mut self,
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        diff_change_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.sync_mut(cx);

        let base_text_buffer_id = diff.read(cx).base_text_buffer().read(cx).remote_id();
        let Some(path) = snapshot.path_for_buffer(base_text_buffer_id).cloned() else {
            return;
        };

        let main_buffer_snapshot = main_buffer.read(cx).snapshot();
        let diff = diff.read(cx);
        let new_diff = DiffStateSnapshot {
            buffer_id: base_text_buffer_id,
            diff: diff.snapshot(cx),
            main_buffer: Some(main_buffer_snapshot),
        };
        let snapshot = self.snapshot.get_mut();
        snapshot.diffs.insert_or_replace(new_diff, ());

        let Some(diff_change_range) = diff_change_range else {
            return;
        };

        let excerpt_edits = snapshot.excerpt_edits_for_diff_change(&path, diff_change_range);
        let edits = Self::sync_diff_transforms(
            snapshot,
            excerpt_edits,
            DiffChangeKind::DiffUpdated {
                // We don't read this field for inverted diffs.
                base_changed: false,
            },
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    pub fn all_buffers_iter(&self) -> impl Iterator<Item = Entity<Buffer>> {
        self.buffers.values().map(|state| state.buffer.clone())
    }

    pub fn all_buffers(&self) -> HashSet<Entity<Buffer>> {
        self.all_buffers_iter().collect()
    }

    pub fn buffer(&self, buffer_id: BufferId) -> Option<Entity<Buffer>> {
        self.buffers
            .get(&buffer_id)
            .map(|state| state.buffer.clone())
    }

    pub fn language_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<Language>> {
        self.point_to_buffer_offset(point, cx)
            .and_then(|(buffer, offset)| buffer.read(cx).language_at(offset))
    }

    pub fn language_settings<'a>(&'a self, cx: &'a App) -> Cow<'a, LanguageSettings> {
        let snapshot = self.snapshot(cx);
        snapshot
            .excerpts
            .first()
            .and_then(|excerpt| self.buffer(excerpt.range.context.start.buffer_id))
            .map(|buffer| LanguageSettings::for_buffer(&buffer.read(cx), cx))
            .unwrap_or_else(move || self.language_settings_at(MultiBufferOffset::default(), cx))
    }

    pub fn language_settings_at<'a, T: ToOffset>(
        &'a self,
        point: T,
        cx: &'a App,
    ) -> Cow<'a, LanguageSettings> {
        if let Some((buffer, offset)) = self.point_to_buffer_offset(point, cx) {
            LanguageSettings::for_buffer_at(buffer.read(cx), offset, cx)
        } else {
            Cow::Borrowed(&AllLanguageSettings::get_global(cx).defaults)
        }
    }

    pub fn for_each_buffer(&self, f: &mut dyn FnMut(&Entity<Buffer>)) {
        self.buffers.values().for_each(|state| f(&state.buffer))
    }

    pub fn explicit_title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn title<'a>(&'a self, cx: &'a App) -> Cow<'a, str> {
        if let Some(title) = self.title.as_ref() {
            return title.into();
        }

        if let Some(buffer) = self.as_singleton() {
            let buffer = buffer.read(cx);

            if let Some(file) = buffer.file() {
                return file.file_name(cx).into();
            }

            if let Some(title) = self.buffer_content_title(buffer) {
                return title;
            }
        };

        "untitled".into()
    }

    pub(super) fn buffer_content_title(&self, buffer: &Buffer) -> Option<Cow<'_, str>> {
        let mut is_leading_whitespace = true;
        let mut count = 0;
        let mut prev_was_space = false;
        let mut title = String::new();

        for ch in buffer.snapshot().chars() {
            if is_leading_whitespace && ch.is_whitespace() {
                continue;
            }

            is_leading_whitespace = false;

            if ch == '\n' || count >= 40 {
                break;
            }

            if ch.is_whitespace() {
                if !prev_was_space {
                    title.push(' ');
                    count += 1;
                    prev_was_space = true;
                }
            } else {
                title.push(ch);
                count += 1;
                prev_was_space = false;
            }
        }

        let title = title.trim_end().to_string();

        if title.is_empty() {
            return None;
        }

        Some(title.into())
    }

    pub fn set_title(&mut self, title: String, cx: &mut Context<Self>) {
        self.title = Some(title);
        cx.notify();
    }

    /// Preserve preview tabs containing this multibuffer until additional edits occur.
    pub fn refresh_preview(&self, cx: &mut Context<Self>) {
        for buffer_state in self.buffers.values() {
            buffer_state
                .buffer
                .update(cx, |buffer, _cx| buffer.refresh_preview());
        }
    }

    /// Whether we should preserve the preview status of a tab containing this multi-buffer.
    pub fn preserve_preview(&self, cx: &App) -> bool {
        self.buffers
            .values()
            .all(|state| state.buffer.read(cx).preserve_preview())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn is_parsing(&self, cx: &App) -> bool {
        self.as_singleton().unwrap().read(cx).is_parsing()
    }
}
