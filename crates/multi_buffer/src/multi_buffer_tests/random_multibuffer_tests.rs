use super::*;

#[gpui::test(iterations = 100)]
async fn test_random_multibuffer(cx: &mut TestAppContext, mut rng: StdRng) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let mut buffers: Vec<Entity<Buffer>> = Vec::new();
    let mut base_texts: HashMap<BufferId, String> = HashMap::default();
    let mut reference = ReferenceMultibuffer::default();
    let mut anchors = Vec::new();
    let mut old_versions = Vec::new();
    let mut needs_diff_calculation = false;
    let mut inverted_diff_main_buffers: HashMap<BufferId, Entity<BufferDiff>> = HashMap::default();
    for _ in 0..operations {
        match rng.random_range(0..100) {
            0..=14 if !buffers.is_empty() => {
                let buffer = buffers.choose(&mut rng).unwrap();
                buffer.update(cx, |buf, cx| {
                    let edit_count = rng.random_range(1..5);
                    buf.randomly_edit(&mut rng, edit_count, cx);
                    log::info!("buffer text:\n{}", buf.text());
                    needs_diff_calculation = true;
                });
                cx.update(|cx| reference.diffs_updated(cx));
            }
            15..=24 if !reference.excerpts.is_empty() => {
                multibuffer.update(cx, |multibuffer, cx| {
                    let snapshot = multibuffer.snapshot(cx);
                    let infos = snapshot.excerpts().collect::<Vec<_>>();
                    let mut excerpts = HashSet::default();
                    for _ in 0..rng.random_range(0..infos.len()) {
                        excerpts.extend(infos.choose(&mut rng).cloned());
                    }

                    let line_count = rng.random_range(0..5);

                    let excerpt_ixs = excerpts
                        .iter()
                        .map(|info| {
                            reference
                                .excerpts
                                .iter()
                                .position(|e| e.range == info.context)
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    log::info!("Expanding excerpts {excerpt_ixs:?} by {line_count} lines");
                    multibuffer.expand_excerpts(
                        excerpts
                            .iter()
                            .map(|info| snapshot.anchor_in_excerpt(info.context.end).unwrap()),
                        line_count,
                        ExpandExcerptDirection::UpAndDown,
                        cx,
                    );

                    reference.expand_excerpts(&excerpts, line_count, cx);
                });
            }
            25..=34 if !reference.excerpts.is_empty() => {
                let multibuffer =
                    multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
                let offset = multibuffer.clip_offset(
                    MultiBufferOffset(rng.random_range(0..=multibuffer.len().0)),
                    Bias::Left,
                );
                let bias = if rng.random() {
                    Bias::Left
                } else {
                    Bias::Right
                };
                log::info!("Creating anchor at {} with bias {:?}", offset.0, bias);
                anchors.push(multibuffer.anchor_at(offset, bias));
                anchors.sort_by(|a, b| a.cmp(b, &multibuffer));
            }
            35..=45 if !reference.excerpts.is_empty() => {
                multibuffer.update(cx, |multibuffer, cx| {
                    let snapshot = multibuffer.snapshot(cx);
                    let excerpt_ix = rng.random_range(0..reference.excerpts.len());
                    let excerpt = &reference.excerpts[excerpt_ix];

                    // Skip inverted excerpts - hunks can't be collapsed
                    let buffer_id = excerpt.buffer.read(cx).remote_id();
                    if reference.inverted_diffs.contains_key(&buffer_id) {
                        return;
                    }

                    let start = excerpt.range.start;
                    let end = excerpt.range.end;
                    let range = snapshot.anchor_in_excerpt(start).unwrap()
                        ..snapshot.anchor_in_excerpt(end).unwrap();

                    log::info!(
                        "expanding diff hunks in range {:?} (excerpt index {excerpt_ix:?}, buffer id {:?})",
                        range.to_point(&snapshot),
                        buffer_id,
                    );
                    reference.expand_diff_hunks(excerpt.path_key.clone(), start..end, cx);
                    multibuffer.expand_diff_hunks(vec![range], cx);
                });
            }
            46..=75 if needs_diff_calculation => {
                multibuffer.update(cx, |multibuffer, cx| {
                    for buffer in multibuffer.all_buffers() {
                        let snapshot = buffer.read(cx).snapshot();
                        let buffer_id = snapshot.remote_id();

                        if let Some(diff) = multibuffer.diff_for(buffer_id) {
                            diff.update(cx, |diff, cx| {
                                log::info!("recalculating diff for buffer {:?}", buffer_id,);
                                diff.recalculate_diff_sync(&snapshot.text, cx);
                            });
                        }

                        if let Some(inverted_diff) = inverted_diff_main_buffers.get(&buffer_id) {
                            inverted_diff.update(cx, |diff, cx| {
                                log::info!(
                                    "recalculating inverted diff for main buffer {:?}",
                                    buffer_id,
                                );
                                diff.recalculate_diff_sync(&snapshot.text, cx);
                            });
                        }
                    }
                    reference.diffs_updated(cx);
                    needs_diff_calculation = false;
                });
            }
            _ => {
                // Decide if we're creating a new buffer or reusing an existing one
                let create_new_buffer = buffers.is_empty() || rng.random_bool(0.4);

                let (excerpt_buffer, diff, inverted_main_buffer) = if create_new_buffer {
                    let create_inverted = rng.random_bool(0.3);

                    if create_inverted {
                        let mut main_buffer_text = util::RandomCharIter::new(&mut rng)
                            .take(256)
                            .collect::<String>();
                        let main_buffer = cx.new(|cx| Buffer::local(main_buffer_text.clone(), cx));
                        text::LineEnding::normalize(&mut main_buffer_text);
                        let main_buffer_id =
                            main_buffer.read_with(cx, |buffer, _| buffer.remote_id());
                        base_texts.insert(main_buffer_id, main_buffer_text.clone());
                        buffers.push(main_buffer.clone());

                        let diff = cx.new(|cx| {
                            BufferDiff::new_with_base_text(
                                &main_buffer_text,
                                &main_buffer.read(cx).text_snapshot(),
                                cx,
                            )
                        });

                        let base_text_buffer =
                            diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

                        // Track for recalculation when main buffer is edited
                        inverted_diff_main_buffers.insert(main_buffer_id, diff.clone());

                        (base_text_buffer, diff, Some(main_buffer))
                    } else {
                        let mut base_text = util::RandomCharIter::new(&mut rng)
                            .take(256)
                            .collect::<String>();

                        let buffer_handle = cx.new(|cx| Buffer::local(base_text.clone(), cx));
                        text::LineEnding::normalize(&mut base_text);
                        let buffer_id = buffer_handle.read_with(cx, |buffer, _| buffer.remote_id());
                        base_texts.insert(buffer_id, base_text.clone());
                        buffers.push(buffer_handle.clone());

                        let diff = cx.new(|cx| {
                            BufferDiff::new_with_base_text(
                                &base_text,
                                &buffer_handle.read(cx).text_snapshot(),
                                cx,
                            )
                        });

                        (buffer_handle, diff, None)
                    }
                } else {
                    // Reuse an existing buffer
                    let buffer_handle = buffers.choose(&mut rng).unwrap().clone();
                    let buffer_id = buffer_handle.read_with(cx, |buffer, _| buffer.remote_id());

                    if let Some(diff) = inverted_diff_main_buffers.get(&buffer_id) {
                        let base_text_buffer =
                            diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());
                        (base_text_buffer, diff.clone(), Some(buffer_handle))
                    } else {
                        // Get existing diff or create new one for regular buffer
                        let diff = multibuffer
                            .read_with(cx, |mb, _| mb.diff_for(buffer_id))
                            .unwrap_or_else(|| {
                                let base_text = base_texts.get(&buffer_id).unwrap();
                                cx.new(|cx| {
                                    BufferDiff::new_with_base_text(
                                        base_text,
                                        &buffer_handle.read(cx).text_snapshot(),
                                        cx,
                                    )
                                })
                            });
                        (buffer_handle, diff, None)
                    }
                };

                let excerpt_buffer_snapshot =
                    excerpt_buffer.read_with(cx, |excerpt_buffer, _| excerpt_buffer.snapshot());
                let mut ranges = reference
                    .excerpts
                    .iter()
                    .filter(|excerpt| excerpt.buffer == excerpt_buffer)
                    .map(|excerpt| excerpt.range.to_point(&excerpt_buffer_snapshot))
                    .collect::<Vec<_>>();
                mutate_excerpt_ranges(&mut rng, &mut ranges, &excerpt_buffer_snapshot, 1);
                let ranges = ranges
                    .iter()
                    .cloned()
                    .map(ExcerptRange::new)
                    .collect::<Vec<_>>();
                let path = cx.update(|cx| PathKey::for_buffer(&excerpt_buffer, cx));
                let path_key_index = multibuffer.update(cx, |multibuffer, _| {
                    multibuffer.get_or_create_path_key_index(&path)
                });

                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.set_excerpt_ranges_for_path(
                        path.clone(),
                        excerpt_buffer.clone(),
                        &excerpt_buffer_snapshot,
                        ranges.clone(),
                        cx,
                    )
                });

                cx.update(|cx| {
                    reference.set_excerpts(
                        path,
                        path_key_index,
                        excerpt_buffer.clone(),
                        &excerpt_buffer_snapshot,
                        ranges,
                        cx,
                    )
                });

                let excerpt_buffer_id =
                    excerpt_buffer.read_with(cx, |buffer, _| buffer.remote_id());
                multibuffer.update(cx, |multibuffer, cx| {
                    if multibuffer.diff_for(excerpt_buffer_id).is_none() {
                        if let Some(main_buffer) = inverted_main_buffer {
                            reference.add_inverted_diff(diff.clone(), main_buffer.clone(), cx);
                            multibuffer.add_inverted_diff(diff, main_buffer, cx);
                        } else {
                            reference.add_diff(diff.clone(), cx);
                            multibuffer.add_diff(diff, cx);
                        }
                    }
                });
            }
        }

        if rng.random_bool(0.3) {
            multibuffer.update(cx, |multibuffer, cx| {
                old_versions.push((multibuffer.snapshot(cx), multibuffer.subscribe()));
            })
        }

        multibuffer.read_with(cx, |multibuffer, cx| {
            check_multibuffer(multibuffer, &reference, &anchors, cx, &mut rng);
        });
    }
    let snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    for (old_snapshot, subscription) in old_versions {
        check_multibuffer_edits(&snapshot, &old_snapshot, subscription);
    }
}
