use super::*;

impl Editor {
    pub(crate) fn enable_wrap_selections_in_tag(&self, cx: &App) -> bool {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        for selection in self.selections.disjoint_anchors_arc().iter() {
            if snapshot
                .language_at(selection.start)
                .and_then(|lang| lang.config().wrap_characters.as_ref())
                .is_some()
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn wrap_selections_in_tag(
        &mut self,
        _: &WrapSelectionsInTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }

        let snapshot = self.buffer.read(cx).snapshot(cx);

        let mut edits = Vec::new();
        let mut boundaries = Vec::new();

        for selection in self
            .selections
            .all_adjusted(&self.display_snapshot(cx))
            .iter()
        {
            let Some(wrap_config) = snapshot
                .language_at(selection.start)
                .and_then(|lang| lang.config().wrap_characters.clone())
            else {
                continue;
            };

            let open_tag = format!("{}{}", wrap_config.start_prefix, wrap_config.start_suffix);
            let close_tag = format!("{}{}", wrap_config.end_prefix, wrap_config.end_suffix);

            let start_before = snapshot.anchor_before(selection.start);
            let end_after = snapshot.anchor_after(selection.end);

            edits.push((start_before..start_before, open_tag));
            edits.push((end_after..end_after, close_tag));

            boundaries.push((
                start_before,
                end_after,
                wrap_config.start_prefix.len(),
                wrap_config.end_suffix.len(),
            ));
        }

        if edits.is_empty() {
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            let buffer = this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
                buffer.snapshot(cx)
            });

            let mut new_selections = Vec::with_capacity(boundaries.len() * 2);
            for (start_before, end_after, start_prefix_len, end_suffix_len) in
                boundaries.into_iter()
            {
                let open_offset = start_before.to_offset(&buffer) + start_prefix_len;
                let close_offset = end_after
                    .to_offset(&buffer)
                    .saturating_sub_usize(end_suffix_len);
                new_selections.push(open_offset..open_offset);
                new_selections.push(close_offset..close_offset);
            }

            this.change_selections(Default::default(), window, cx, |s| {
                s.select_ranges(new_selections);
            });

            this.request_autoscroll(Autoscroll::fit(), cx);
        });
    }
}
