use super::*;

#[cfg(any(test, feature = "test-support"))]
impl MultiBuffer {
    pub fn build_simple(text: &str, cx: &mut gpui::App) -> Entity<Self> {
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        cx.new(|cx| Self::singleton(buffer, cx))
    }

    pub fn build_multi<const COUNT: usize>(
        excerpts: [(&str, Vec<Range<Point>>); COUNT],
        cx: &mut gpui::App,
    ) -> Entity<Self> {
        let multi = cx.new(|_| Self::new(Capability::ReadWrite));
        for (ix, (text, ranges)) in excerpts.into_iter().enumerate() {
            let buffer = cx.new(|cx| Buffer::local(text, cx));
            let snapshot = buffer.read(cx).snapshot();
            let excerpt_ranges = ranges
                .into_iter()
                .map(ExcerptRange::new)
                .collect::<Vec<_>>();
            multi.update(cx, |multi, cx| {
                multi.set_excerpt_ranges_for_path(
                    PathKey::sorted(ix as u64),
                    buffer,
                    &snapshot,
                    excerpt_ranges,
                    cx,
                )
            });
        }

        multi
    }

    pub fn build_from_buffer(buffer: Entity<Buffer>, cx: &mut gpui::App) -> Entity<Self> {
        cx.new(|cx| Self::singleton(buffer, cx))
    }

    pub fn build_random(rng: &mut impl rand::Rng, cx: &mut gpui::App) -> Entity<Self> {
        cx.new(|cx| {
            let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
            let mutation_count = rng.random_range(1..=5);
            multibuffer.randomly_edit_excerpts(rng, mutation_count, cx);
            multibuffer
        })
    }

    pub fn randomly_edit(
        &mut self,
        rng: &mut impl rand::Rng,
        edit_count: usize,
        cx: &mut Context<Self>,
    ) {
        use util::RandomCharIter;

        let snapshot = self.read(cx);
        let mut edits: Vec<(Range<MultiBufferOffset>, Arc<str>)> = Vec::new();
        let mut last_end = None;
        for _ in 0..edit_count {
            if last_end.is_some_and(|last_end| last_end >= snapshot.len()) {
                break;
            }

            let new_start = last_end.map_or(MultiBufferOffset::ZERO, |last_end| last_end + 1usize);
            let end =
                snapshot.clip_offset(rng.random_range(new_start..=snapshot.len()), Bias::Right);
            let start = snapshot.clip_offset(rng.random_range(new_start..=end), Bias::Right);
            last_end = Some(end);

            let mut range = start..end;
            if rng.random_bool(0.2) {
                mem::swap(&mut range.start, &mut range.end);
            }

            let new_text_len = rng.random_range(0..10);
            let new_text: String = RandomCharIter::new(&mut *rng).take(new_text_len).collect();

            edits.push((range, new_text.into()));
        }
        log::info!("mutating multi-buffer with {:?}", edits);
        drop(snapshot);

        self.edit(edits, None, cx);
    }

    pub fn randomly_edit_excerpts(
        &mut self,
        rng: &mut impl rand::Rng,
        mutation_count: usize,
        cx: &mut Context<Self>,
    ) {
        use rand::prelude::*;
        use std::env;
        use util::RandomCharIter;

        let max_buffers = env::var("MAX_BUFFERS")
            .map(|i| i.parse().expect("invalid `MAX_EXCERPTS` variable"))
            .unwrap_or(5);

        let mut buffers = Vec::new();
        for _ in 0..mutation_count {
            let snapshot = self.snapshot(cx);
            let buffer_ids = snapshot.all_buffer_ids().collect::<Vec<_>>();
            if buffer_ids.is_empty() || (rng.random() && buffer_ids.len() < max_buffers) {
                let buffer_handle = if rng.random() || self.buffers.is_empty() {
                    let text = RandomCharIter::new(&mut *rng).take(10).collect::<String>();
                    buffers.push(cx.new(|cx| Buffer::local(text, cx)));
                    let buffer = buffers.last().unwrap().read(cx);
                    log::info!(
                        "Creating new buffer {} with text: {:?}",
                        buffer.remote_id(),
                        buffer.text()
                    );
                    buffers.last().unwrap().clone()
                } else {
                    self.buffers.values().choose(rng).unwrap().buffer.clone()
                };

                let buffer = buffer_handle.read(cx);
                let buffer_text = buffer.text();
                let buffer_snapshot = buffer.snapshot();
                let mut next_min_start_ix = 0;
                let ranges = (0..rng.random_range(0..5))
                    .filter_map(|_| {
                        if next_min_start_ix >= buffer.len() {
                            return None;
                        }
                        let end_ix = buffer.clip_offset(
                            rng.random_range(next_min_start_ix..=buffer.len()),
                            Bias::Right,
                        );
                        let start_ix = buffer
                            .clip_offset(rng.random_range(next_min_start_ix..=end_ix), Bias::Left);
                        next_min_start_ix = buffer.text().ceil_char_boundary(end_ix + 1);
                        Some(ExcerptRange::new(start_ix..end_ix))
                    })
                    .collect::<Vec<_>>();
                log::info!(
                    "Inserting excerpts from buffer {} and ranges {:?}: {:?}",
                    buffer_handle.read(cx).remote_id(),
                    ranges.iter().map(|r| &r.context).collect::<Vec<_>>(),
                    ranges
                        .iter()
                        .map(|r| &buffer_text[r.context.clone()])
                        .collect::<Vec<_>>()
                );

                let path_key = PathKey::for_buffer(&buffer_handle, cx);
                self.set_merged_excerpt_ranges_for_path(
                    path_key.clone(),
                    buffer_handle,
                    &buffer_snapshot,
                    ranges,
                    cx,
                );
                log::info!("Inserted with path_key: {:?}", path_key);
            } else {
                let path_key = self
                    .snapshot
                    .borrow()
                    .buffers
                    .get(&buffer_ids.choose(rng).unwrap())
                    .unwrap()
                    .path_key
                    .clone();
                log::info!("Removing excerpts {:?}", path_key);
                self.remove_excerpts(path_key, cx);
            }
        }
    }

    pub fn randomly_mutate(
        &mut self,
        rng: &mut impl rand::Rng,
        mutation_count: usize,
        cx: &mut Context<Self>,
    ) {
        use rand::prelude::*;

        if rng.random_bool(0.7) || self.singleton {
            let buffer = self
                .buffers
                .values()
                .choose(rng)
                .map(|state| state.buffer.clone());

            if let Some(buffer) = buffer {
                buffer.update(cx, |buffer, cx| {
                    if rng.random() {
                        buffer.randomly_edit(rng, mutation_count, cx);
                    } else {
                        buffer.randomly_undo_redo(rng, cx);
                    }
                });
            } else {
                self.randomly_edit(rng, mutation_count, cx);
            }
        } else {
            self.randomly_edit_excerpts(rng, mutation_count, cx);
        }

        self.check_invariants(cx);
    }

    fn check_invariants(&self, cx: &App) {
        self.read(cx).check_invariants();
    }
}
