use super::*;

#[gpui::test(iterations = 25)]
async fn test_random_split_editor(mut rng: StdRng, cx: &mut gpui::TestAppContext) {
    use multi_buffer::ExpandExcerptDirection;
    use rand::prelude::*;
    use util::RandomCharIter;

    let (editor, cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;
    let operations = std::env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);
    let rng = &mut rng;
    for _ in 0..operations {
        let buffers = editor.update(cx, |editor, cx| {
            editor.rhs_editor.read(cx).buffer().read(cx).all_buffers()
        });

        if buffers.is_empty() {
            log::info!("creating initial buffer");
            let len = rng.random_range(200..1000);
            let base_text: String = RandomCharIter::new(&mut *rng).take(len).collect();
            let buffer = cx.new(|cx| Buffer::local(base_text.clone(), cx));
            let buffer_snapshot = buffer.read_with(cx, |b, _| b.text_snapshot());
            let diff =
                cx.new(|cx| BufferDiff::new_with_base_text(&base_text, &buffer_snapshot, cx));
            let edit_count = rng.random_range(3..8);
            buffer.update(cx, |buffer, cx| {
                buffer.randomly_edit(rng, edit_count, cx);
            });
            let buffer_snapshot = buffer.read_with(cx, |b, _| b.text_snapshot());
            diff.update(cx, |diff, cx| {
                diff.recalculate_diff_sync(&buffer_snapshot, cx);
            });
            let diff_snapshot = diff.read_with(cx, |diff, cx| diff.snapshot(cx));
            let ranges = diff_snapshot
                .hunks(&buffer_snapshot)
                .map(|hunk| hunk.range)
                .collect::<Vec<_>>();
            let context_lines = rng.random_range(0..2);
            editor.update(cx, |editor, cx| {
                let path = PathKey::for_buffer(&buffer, cx);
                editor.update_excerpts_for_path(path, buffer, ranges, context_lines, diff, cx);
            });
            editor.update(cx, |editor, cx| {
                editor.check_invariants(true, cx);
            });
            continue;
        }

        let mut quiesced = false;

        match rng.random_range(0..100) {
            0..=14 if buffers.len() < 6 => {
                log::info!("creating new buffer and setting excerpts");
                let len = rng.random_range(200..1000);
                let base_text: String = RandomCharIter::new(&mut *rng).take(len).collect();
                let buffer = cx.new(|cx| Buffer::local(base_text.clone(), cx));
                let buffer_snapshot = buffer.read_with(cx, |b, _| b.text_snapshot());
                let diff =
                    cx.new(|cx| BufferDiff::new_with_base_text(&base_text, &buffer_snapshot, cx));
                let edit_count = rng.random_range(3..8);
                buffer.update(cx, |buffer, cx| {
                    buffer.randomly_edit(rng, edit_count, cx);
                });
                let buffer_snapshot = buffer.read_with(cx, |b, _| b.text_snapshot());
                diff.update(cx, |diff, cx| {
                    diff.recalculate_diff_sync(&buffer_snapshot, cx);
                });
                let diff_snapshot = diff.read_with(cx, |diff, cx| diff.snapshot(cx));
                let ranges = diff_snapshot
                    .hunks(&buffer_snapshot)
                    .map(|hunk| hunk.range)
                    .collect::<Vec<_>>();
                let context_lines = rng.random_range(0..2);
                editor.update(cx, |editor, cx| {
                    let path = PathKey::for_buffer(&buffer, cx);
                    editor.update_excerpts_for_path(path, buffer, ranges, context_lines, diff, cx);
                });
            }
            15..=29 => {
                log::info!("randomly editing multibuffer");
                let edit_count = rng.random_range(1..5);
                editor.update(cx, |editor, cx| {
                    editor.rhs_multibuffer.update(cx, |multibuffer, cx| {
                        multibuffer.randomly_edit(rng, edit_count, cx);
                    });
                });
            }
            30..=44 => {
                log::info!("randomly editing individual buffer");
                let buffer = buffers.iter().choose(rng).unwrap();
                let edit_count = rng.random_range(1..3);
                buffer.update(cx, |buffer, cx| {
                    buffer.randomly_edit(rng, edit_count, cx);
                });
            }
            45..=54 => {
                log::info!("recalculating diff and resetting excerpts for single buffer");
                let buffer = buffers.iter().choose(rng).unwrap();
                let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
                let diff = editor.update(cx, |editor, cx| {
                    editor
                        .rhs_multibuffer
                        .read(cx)
                        .diff_for(buffer.read(cx).remote_id())
                        .unwrap()
                });
                diff.update(cx, |diff, cx| {
                    diff.recalculate_diff_sync(&buffer_snapshot, cx);
                });
                cx.run_until_parked();
                let diff_snapshot = diff.read_with(cx, |diff, cx| diff.snapshot(cx));
                let ranges = diff_snapshot
                    .hunks(&buffer_snapshot)
                    .map(|hunk| hunk.range)
                    .collect::<Vec<_>>();
                let context_lines = rng.random_range(0..2);
                let buffer = buffer.clone();
                editor.update(cx, |editor, cx| {
                    let path = PathKey::for_buffer(&buffer, cx);
                    editor.update_excerpts_for_path(path, buffer, ranges, context_lines, diff, cx);
                });
            }
            55..=64 => {
                log::info!("randomly undoing/redoing in single buffer");
                let buffer = buffers.iter().choose(rng).unwrap();
                buffer.update(cx, |buffer, cx| {
                    buffer.randomly_undo_redo(rng, cx);
                });
            }
            65..=74 => {
                log::info!("removing excerpts for a random path");
                let ids = editor.update(cx, |editor, cx| {
                    let snapshot = editor.rhs_multibuffer.read(cx).snapshot(cx);
                    snapshot.all_buffer_ids().collect::<Vec<_>>()
                });
                if let Some(id) = ids.choose(rng) {
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.rhs_multibuffer.read(cx).snapshot(cx);
                        let path = snapshot.path_for_buffer(*id).unwrap();
                        editor.remove_excerpts_for_path(path.clone(), cx);
                    });
                }
            }
            75..=79 => {
                log::info!("unsplit, scroll stale lhs, and resplit");
                let Some(lhs_editor) = editor.update(cx, |editor, _cx| {
                    editor.lhs.as_ref().map(|lhs| lhs.editor.clone())
                }) else {
                    continue;
                };
                let lhs_max_row = lhs_editor.update(cx, |editor, cx| {
                    editor.display_snapshot(cx).max_point().row().0
                });
                editor.update_in(cx, |editor, window, cx| {
                    editor.unsplit(window, cx);
                });
                cx.run_until_parked();

                if lhs_max_row > 0 {
                    lhs_editor.update_in(cx, |editor, window, cx| {
                        editor.set_scroll_position(gpui::Point::new(0., 1.), window, cx);
                    });
                    editor.update(cx, |editor, cx| {
                        editor.check_invariants(false, cx);
                    });
                }

                editor.update_in(cx, |editor, window, cx| {
                    editor.split(window, cx);
                });
            }
            80..=89 => {
                let snapshot = editor.update(cx, |editor, cx| {
                    editor.rhs_multibuffer.read(cx).snapshot(cx)
                });
                let excerpts = snapshot.excerpts().collect::<Vec<_>>();
                if !excerpts.is_empty() {
                    let count = rng.random_range(1..=excerpts.len().min(3));
                    let chosen: Vec<_> = excerpts.choose_multiple(rng, count).cloned().collect();
                    let line_count = rng.random_range(1..5);
                    log::info!("expanding {count} excerpts by {line_count} lines");
                    editor.update(cx, |editor, cx| {
                        editor.expand_excerpts(
                            chosen.into_iter().map(|excerpt| {
                                snapshot.anchor_in_excerpt(excerpt.context.start).unwrap()
                            }),
                            line_count,
                            ExpandExcerptDirection::UpAndDown,
                            cx,
                        );
                    });
                }
            }
            _ => {
                log::info!("quiescing");
                for buffer in buffers {
                    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
                    let diff = editor.update(cx, |editor, cx| {
                        editor
                            .rhs_multibuffer
                            .read(cx)
                            .diff_for(buffer.read(cx).remote_id())
                            .unwrap()
                    });
                    diff.update(cx, |diff, cx| {
                        diff.recalculate_diff_sync(&buffer_snapshot, cx);
                    });
                    cx.run_until_parked();
                    let diff_snapshot = diff.read_with(cx, |diff, cx| diff.snapshot(cx));
                    let ranges = diff_snapshot
                        .hunks(&buffer_snapshot)
                        .map(|hunk| hunk.range)
                        .collect::<Vec<_>>();
                    editor.update(cx, |editor, cx| {
                        let path = PathKey::for_buffer(&buffer, cx);
                        editor.update_excerpts_for_path(path, buffer, ranges, 2, diff, cx);
                    });
                }
                quiesced = true;
            }
        }

        editor.update(cx, |editor, cx| {
            editor.check_invariants(quiesced, cx);
        });
    }
}
