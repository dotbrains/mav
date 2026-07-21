use super::*;

impl ReferenceMultibuffer {
    pub(super) fn expected_content(
        &self,
        cx: &App,
    ) -> (
        String,
        Vec<RowInfo>,
        HashSet<MultiBufferRow>,
        Vec<ReferenceRegion>,
    ) {
        use util::maybe;

        let mut text = String::new();
        let mut regions = Vec::<ReferenceRegion>::new();
        let mut excerpt_boundary_rows = HashSet::default();
        for excerpt in &self.excerpts {
            excerpt_boundary_rows.insert(MultiBufferRow(text.matches('\n').count() as u32));
            let buffer = excerpt.buffer.read(cx);
            let buffer_id = buffer.remote_id();
            let buffer_range = excerpt.range.to_offset(buffer);

            if let Some((diff, main_buffer)) = self.inverted_diffs.get(&buffer_id) {
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let main_buffer_snapshot = main_buffer.read(cx).snapshot();

                let mut offset = buffer_range.start;
                for hunk in diff_snapshot.hunks_intersecting_base_text_range(
                    buffer_range.clone(),
                    &main_buffer_snapshot.text,
                ) {
                    let mut hunk_base_range = hunk.diff_base_byte_range.clone();

                    hunk_base_range.end = hunk_base_range.end.min(buffer_range.end);
                    if hunk_base_range.start > buffer_range.end
                        || hunk_base_range.start < buffer_range.start
                    {
                        continue;
                    }

                    // Add the text before the hunk
                    if hunk_base_range.start >= offset {
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_base_range.start));
                        if text.len() > len {
                            regions.push(ReferenceRegion {
                                buffer_id: Some(buffer_id),
                                range: len..text.len(),
                                buffer_range: (offset..hunk_base_range.start).to_point(&buffer),
                                status: None,
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: None,
                            });
                        }
                    }

                    // Add the "deleted" region (base text that's not in main)
                    if !hunk_base_range.is_empty() {
                        let len = text.len();
                        text.extend(buffer.text_for_range(hunk_base_range.clone()));
                        regions.push(ReferenceRegion {
                            buffer_id: Some(buffer_id),
                            range: len..text.len(),
                            buffer_range: hunk_base_range.to_point(&buffer),
                            status: Some(DiffHunkStatus::deleted(hunk.secondary_status)),
                            excerpt: Some(excerpt.clone()),
                            deleted_hunk_anchor: None,
                        });
                    }

                    offset = hunk_base_range.end;
                }

                // Add remaining buffer text
                let len = text.len();
                text.extend(buffer.text_for_range(offset..buffer_range.end));
                text.push('\n');
                regions.push(ReferenceRegion {
                    buffer_id: Some(buffer_id),
                    range: len..text.len(),
                    buffer_range: (offset..buffer_range.end).to_point(&buffer),
                    status: None,
                    excerpt: Some(excerpt.clone()),
                    deleted_hunk_anchor: None,
                });
            } else {
                let diff = self.diffs.get(&buffer_id).unwrap().read(cx).snapshot(cx);
                let base_buffer = diff.base_text();

                let mut offset = buffer_range.start;
                let hunks = diff
                    .hunks_intersecting_range(excerpt.range.clone(), buffer)
                    .peekable();

                for hunk in hunks {
                    // Ignore hunks that are outside the excerpt range.
                    let mut hunk_range = hunk.buffer_range.to_offset(buffer);

                    hunk_range.end = hunk_range.end.min(buffer_range.end);
                    if hunk_range.start > buffer_range.end || hunk_range.start < buffer_range.start
                    {
                        log::trace!("skipping hunk outside excerpt range");
                        continue;
                    }

                    if !self
                        .expanded_diff_hunks_by_buffer
                        .get(&buffer_id)
                        .cloned()
                        .into_iter()
                        .flatten()
                        .any(|expanded_anchor| {
                            expanded_anchor
                                .cmp(&hunk.buffer_range.start, buffer)
                                .is_eq()
                        })
                    {
                        log::trace!("skipping a hunk that's not marked as expanded");
                        continue;
                    }

                    if !hunk.buffer_range.start.is_valid(buffer) {
                        log::trace!("skipping hunk with deleted start: {:?}", hunk.range);
                        continue;
                    }

                    if hunk_range.start >= offset {
                        // Add the buffer text before the hunk
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_range.start));
                        if text.len() > len {
                            regions.push(ReferenceRegion {
                                buffer_id: Some(buffer_id),
                                range: len..text.len(),
                                buffer_range: (offset..hunk_range.start).to_point(&buffer),
                                status: None,
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: None,
                            });
                        }

                        // Add the deleted text for the hunk.
                        if !hunk.diff_base_byte_range.is_empty() {
                            let mut base_text = base_buffer
                                .text_for_range(hunk.diff_base_byte_range.clone())
                                .collect::<String>();
                            if !base_text.ends_with('\n') {
                                base_text.push('\n');
                            }
                            let len = text.len();
                            text.push_str(&base_text);
                            regions.push(ReferenceRegion {
                                buffer_id: Some(base_buffer.remote_id()),
                                range: len..text.len(),
                                buffer_range: hunk.diff_base_byte_range.to_point(&base_buffer),
                                status: Some(DiffHunkStatus::deleted(hunk.secondary_status)),
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: Some(hunk.buffer_range.start),
                            });
                        }

                        offset = hunk_range.start;
                    }

                    // Add the inserted text for the hunk.
                    if hunk_range.end > offset {
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_range.end));
                        let range = len..text.len();
                        let region = ReferenceRegion {
                            buffer_id: Some(buffer_id),
                            range,
                            buffer_range: (offset..hunk_range.end).to_point(&buffer),
                            status: Some(DiffHunkStatus::added(hunk.secondary_status)),
                            excerpt: Some(excerpt.clone()),
                            deleted_hunk_anchor: None,
                        };
                        offset = hunk_range.end;
                        regions.push(region);
                    }
                }

                // Add the buffer text for the rest of the excerpt.
                let len = text.len();
                text.extend(buffer.text_for_range(offset..buffer_range.end));
                text.push('\n');
                regions.push(ReferenceRegion {
                    buffer_id: Some(buffer_id),
                    range: len..text.len(),
                    buffer_range: (offset..buffer_range.end).to_point(&buffer),
                    status: None,
                    excerpt: Some(excerpt.clone()),
                    deleted_hunk_anchor: None,
                });
            }
        }

        // Remove final trailing newline.
        if self.excerpts.is_empty() {
            regions.push(ReferenceRegion {
                buffer_id: None,
                range: 0..1,
                buffer_range: Point::new(0, 0)..Point::new(0, 1),
                status: None,
                excerpt: None,
                deleted_hunk_anchor: None,
            });
        } else {
            text.pop();
            let region = regions.last_mut().unwrap();
            assert!(region.deleted_hunk_anchor.is_none());
            region.range.end -= 1;
        }

        // Retrieve the row info using the region that contains
        // the start of each multi-buffer line.
        let mut ix = 0;
        let row_infos = text
            .split('\n')
            .map(|line| {
                let row_info = regions
                    .iter()
                    .rposition(|region| {
                        region.range.contains(&ix) || (ix == text.len() && ix == region.range.end)
                    })
                    .map_or(RowInfo::default(), |region_ix| {
                        let region = regions[region_ix].clone();
                        let buffer_row = region.buffer_range.start.row
                            + text[region.range.start..ix].matches('\n').count() as u32;
                        let main_buffer = region.excerpt.as_ref().map(|e| e.buffer.clone());
                        let excerpt_range = region.excerpt.as_ref().map(|e| &e.range);
                        let is_excerpt_start = region_ix == 0
                            || regions[region_ix - 1].excerpt.as_ref().map(|e| &e.range)
                                != excerpt_range
                            || regions[region_ix - 1].range.is_empty();
                        let mut is_excerpt_end = region_ix == regions.len() - 1
                            || regions[region_ix + 1].excerpt.as_ref().map(|e| &e.range)
                                != excerpt_range;
                        let is_start = !text[region.range.start..ix].contains('\n');
                        let is_last_region = region_ix == regions.len() - 1;
                        let mut is_end = if region.range.end > text.len() {
                            !text[ix..].contains('\n')
                        } else {
                            let remaining_newlines = text[ix..region.range.end.min(text.len())]
                                .matches('\n')
                                .count();
                            remaining_newlines == if is_last_region { 0 } else { 1 }
                        };
                        if region_ix < regions.len() - 1
                            && !text[ix..].contains("\n")
                            && (region.status == Some(DiffHunkStatus::added_none())
                                || region.status.is_some_and(|s| s.is_deleted()))
                            && regions[region_ix + 1].excerpt.as_ref().map(|e| &e.range)
                                == excerpt_range
                            && regions[region_ix + 1].range.start == text.len()
                        {
                            is_end = true;
                            is_excerpt_end = true;
                        }
                        let multibuffer_row =
                            MultiBufferRow(text[..ix].matches('\n').count() as u32);
                        let mut expand_direction = None;
                        if let Some(buffer) = &main_buffer {
                            let needs_expand_up = is_excerpt_start && is_start && buffer_row > 0;
                            let needs_expand_down = is_excerpt_end
                                && is_end
                                && buffer.read(cx).max_point().row > buffer_row;
                            expand_direction = if needs_expand_up && needs_expand_down {
                                Some(ExpandExcerptDirection::UpAndDown)
                            } else if needs_expand_up {
                                Some(ExpandExcerptDirection::Up)
                            } else if needs_expand_down {
                                Some(ExpandExcerptDirection::Down)
                            } else {
                                None
                            };
                        }
                        RowInfo {
                            buffer_id: region.buffer_id,
                            diff_status: region.status,
                            buffer_row: Some(buffer_row),
                            wrapped_buffer_row: None,

                            multibuffer_row: Some(multibuffer_row),
                            expand_info: maybe!({
                                let direction = expand_direction?;
                                let excerpt = region.excerpt.as_ref()?;
                                Some(ExpandInfo {
                                    direction,
                                    start_anchor: Anchor::in_buffer(
                                        excerpt.path_key_index,
                                        excerpt.range.start,
                                    ),
                                })
                            }),
                        }
                    });
                ix += line.len() + 1;
                row_info
            })
            .collect();

        (text, row_infos, excerpt_boundary_rows, regions)
    }
}
