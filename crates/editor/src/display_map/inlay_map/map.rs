use super::*;

impl InlayMap {
    #[ztracing::instrument(skip_all)]
    pub fn new(buffer: MultiBufferSnapshot) -> (Self, InlaySnapshot) {
        let version = 0;
        let snapshot = InlaySnapshot {
            transforms: SumTree::from_iter(
                iter::once(Transform::Isomorphic(buffer.text_summary())),
                (),
            ),
            buffer,
            version,
        };

        (
            Self {
                snapshot: snapshot.clone(),
                inlays: Vec::new(),
            },
            snapshot,
        )
    }

    #[ztracing::instrument(skip_all)]
    pub fn sync(
        &mut self,
        buffer_snapshot: MultiBufferSnapshot,
        mut buffer_edits: Vec<text::Edit<MultiBufferOffset>>,
    ) -> (InlaySnapshot, Vec<InlayEdit>) {
        let snapshot = &mut self.snapshot;

        if buffer_edits.is_empty()
            && snapshot.buffer.trailing_excerpt_update_count()
                != buffer_snapshot.trailing_excerpt_update_count()
        {
            buffer_edits.push(Edit {
                old: snapshot.buffer.len()..snapshot.buffer.len(),
                new: buffer_snapshot.len()..buffer_snapshot.len(),
            });
        }

        if buffer_edits.is_empty() {
            if snapshot.buffer.edit_count() != buffer_snapshot.edit_count()
                || snapshot.buffer.non_text_state_update_count()
                    != buffer_snapshot.non_text_state_update_count()
                || snapshot.buffer.trailing_excerpt_update_count()
                    != buffer_snapshot.trailing_excerpt_update_count()
            {
                snapshot.version += 1;
            }

            snapshot.buffer = buffer_snapshot;
            (snapshot.clone(), Vec::new())
        } else if self.inlays.is_empty() && !snapshot.transforms.summary().has_inlays() {
            // Fast path: without inlays, the InlayMap is a passthrough, so rebuild a single
            // isomorphic transform and forward buffer edits as inlay edits verbatim.
            let mut new_transforms = SumTree::default();
            push_isomorphic(&mut new_transforms, buffer_snapshot.text_summary());
            if new_transforms.is_empty() {
                new_transforms.push(Transform::Isomorphic(Default::default()), ());
            }

            let mut inlay_edits = Patch::default();
            for buffer_edit in &buffer_edits {
                inlay_edits.push(Edit {
                    old: InlayOffset(buffer_edit.old.start)..InlayOffset(buffer_edit.old.end),
                    new: InlayOffset(buffer_edit.new.start)..InlayOffset(buffer_edit.new.end),
                });
            }

            snapshot.transforms = new_transforms;
            snapshot.version += 1;
            snapshot.buffer = buffer_snapshot;
            snapshot.check_invariants();

            (snapshot.clone(), inlay_edits.into_inner())
        } else {
            let mut inlay_edits = Patch::default();
            let mut new_transforms = SumTree::default();
            let mut cursor = snapshot
                .transforms
                .cursor::<Dimensions<MultiBufferOffset, InlayOffset>>(());
            let mut buffer_edits_iter = buffer_edits.iter().peekable();
            while let Some(buffer_edit) = buffer_edits_iter.next() {
                new_transforms.append(cursor.slice(&buffer_edit.old.start, Bias::Left), ());
                if let Some(Transform::Isomorphic(transform)) = cursor.item()
                    && cursor.end().0 == buffer_edit.old.start
                {
                    push_isomorphic(&mut new_transforms, *transform);
                    cursor.next();
                }

                // Remove all the inlays and transforms contained by the edit.
                let old_start = cursor.start().1 + (buffer_edit.old.start - cursor.start().0);
                cursor.seek(&buffer_edit.old.end, Bias::Right);
                let old_end = cursor.start().1 + (buffer_edit.old.end - cursor.start().0);

                // Push the unchanged prefix.
                let prefix_start = new_transforms.summary().input.len;
                let prefix_end = buffer_edit.new.start;
                push_isomorphic(
                    &mut new_transforms,
                    buffer_snapshot.text_summary_for_range(prefix_start..prefix_end),
                );
                let new_start = InlayOffset(new_transforms.summary().output.len);

                let start_ix = match self.inlays.binary_search_by(|probe| {
                    probe
                        .position
                        .to_offset(&buffer_snapshot)
                        .cmp(&buffer_edit.new.start)
                        .then(std::cmp::Ordering::Greater)
                }) {
                    Ok(ix) | Err(ix) => ix,
                };

                for inlay in &self.inlays[start_ix..] {
                    if !inlay.position.is_valid(&buffer_snapshot) {
                        continue;
                    }
                    let buffer_offset = inlay.position.to_offset(&buffer_snapshot);
                    if buffer_offset > buffer_edit.new.end {
                        break;
                    }

                    let prefix_start = new_transforms.summary().input.len;
                    let prefix_end = buffer_offset;
                    push_isomorphic(
                        &mut new_transforms,
                        buffer_snapshot.text_summary_for_range(prefix_start..prefix_end),
                    );

                    new_transforms.push(Transform::Inlay(inlay.clone()), ());
                }

                // Apply the rest of the edit.
                let transform_start = new_transforms.summary().input.len;
                push_isomorphic(
                    &mut new_transforms,
                    buffer_snapshot.text_summary_for_range(transform_start..buffer_edit.new.end),
                );
                let new_end = InlayOffset(new_transforms.summary().output.len);
                inlay_edits.push(Edit {
                    old: old_start..old_end,
                    new: new_start..new_end,
                });

                // If the next edit doesn't intersect the current isomorphic transform, then
                // we can push its remainder.
                if buffer_edits_iter
                    .peek()
                    .is_none_or(|edit| edit.old.start >= cursor.end().0)
                {
                    let transform_start = new_transforms.summary().input.len;
                    let transform_end =
                        buffer_edit.new.end + (cursor.end().0 - buffer_edit.old.end);
                    push_isomorphic(
                        &mut new_transforms,
                        buffer_snapshot.text_summary_for_range(transform_start..transform_end),
                    );
                    cursor.next();
                }
            }

            new_transforms.append(cursor.suffix(), ());
            if new_transforms.is_empty() {
                new_transforms.push(Transform::Isomorphic(Default::default()), ());
            }

            drop(cursor);
            snapshot.transforms = new_transforms;
            snapshot.version += 1;
            snapshot.buffer = buffer_snapshot;
            snapshot.check_invariants();

            (snapshot.clone(), inlay_edits.into_inner())
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn splice(
        &mut self,
        to_remove: &[InlayId],
        to_insert: Vec<Inlay>,
    ) -> (InlaySnapshot, Vec<InlayEdit>) {
        let snapshot = &mut self.snapshot;
        let mut edits = BTreeSet::new();

        self.inlays.retain(|inlay| {
            let retain = !to_remove.contains(&inlay.id);
            if !retain {
                let offset = inlay.position.to_offset(&snapshot.buffer);
                edits.insert(offset);
            }
            retain
        });

        for inlay_to_insert in to_insert {
            // Avoid inserting empty inlays.
            if inlay_to_insert.text().is_empty() {
                continue;
            }

            let offset = inlay_to_insert.position.to_offset(&snapshot.buffer);
            match self.inlays.binary_search_by(|probe| {
                probe
                    .position
                    .cmp(&inlay_to_insert.position, &snapshot.buffer)
                    .then(std::cmp::Ordering::Less)
            }) {
                Ok(ix) | Err(ix) => {
                    self.inlays.insert(ix, inlay_to_insert);
                }
            }

            edits.insert(offset);
        }

        let buffer_edits = edits
            .into_iter()
            .map(|offset| Edit {
                old: offset..offset,
                new: offset..offset,
            })
            .collect();
        let buffer_snapshot = snapshot.buffer.clone();
        let (snapshot, edits) = self.sync(buffer_snapshot, buffer_edits);
        (snapshot, edits)
    }

    #[ztracing::instrument(skip_all)]
    pub fn current_inlays(&self) -> impl Iterator<Item = &Inlay> + Default {
        self.inlays.iter()
    }

    #[cfg(test)]
    #[ztracing::instrument(skip_all)]
    pub(crate) fn randomly_mutate(
        &mut self,
        next_inlay_id: &mut usize,
        rng: &mut rand::rngs::StdRng,
    ) -> (InlaySnapshot, Vec<InlayEdit>) {
        use rand::prelude::*;
        use util::post_inc;

        let mut to_remove = Vec::new();
        let mut to_insert = Vec::new();
        let snapshot = &mut self.snapshot;
        for i in 0..rng.random_range(1..=5) {
            if self.inlays.is_empty() || rng.random() {
                let position = snapshot
                    .buffer
                    .random_byte_range(MultiBufferOffset(0), rng)
                    .start;
                let bias = if rng.random() {
                    Bias::Left
                } else {
                    Bias::Right
                };
                let len = if rng.random_bool(0.01) {
                    0
                } else {
                    rng.random_range(1..=5)
                };
                let text = util::RandomCharIter::new(&mut *rng)
                    .filter(|ch| *ch != '\r')
                    .take(len)
                    .collect::<String>();

                let next_inlay = if i % 2 == 0 {
                    Inlay::mock_hint(
                        post_inc(next_inlay_id),
                        snapshot.buffer.anchor_at(position, bias),
                        &text,
                    )
                } else {
                    Inlay::edit_prediction(
                        post_inc(next_inlay_id),
                        snapshot.buffer.anchor_at(position, bias),
                        &text,
                    )
                };
                let inlay_id = next_inlay.id;
                log::info!(
                    "creating inlay {inlay_id:?} at buffer offset {position} with bias {bias:?} and text {text:?}"
                );
                to_insert.push(next_inlay);
            } else {
                to_remove.push(
                    self.inlays
                        .iter()
                        .choose(rng)
                        .map(|inlay| inlay.id)
                        .unwrap(),
                );
            }
        }
        log::info!("removing inlays: {:?}", to_remove);

        let (snapshot, edits) = self.splice(&to_remove, to_insert);
        (snapshot, edits)
    }
}
