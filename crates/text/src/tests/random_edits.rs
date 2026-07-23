use super::*;

fn test_random_edits(mut rng: StdRng) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let reference_string_len = rng.random_range(0..3);
    let mut reference_string = RandomCharIter::new(&mut rng)
        .take(reference_string_len)
        .collect::<String>();
    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        reference_string.clone(),
    );
    LineEnding::normalize(&mut reference_string);

    buffer.set_group_interval(Duration::from_millis(rng.random_range(0..=200)));
    let mut buffer_versions = Vec::new();
    log::info!(
        "buffer text {:?}, version: {:?}",
        buffer.text(),
        buffer.version()
    );

    for _i in 0..operations {
        let (edits, _) = buffer.randomly_edit(&mut rng, 5);
        for (old_range, new_text) in edits.iter().rev() {
            reference_string.replace_range(old_range.clone(), new_text);
        }

        assert_eq!(buffer.text(), reference_string);
        log::info!(
            "buffer text {:?}, version: {:?}",
            buffer.text(),
            buffer.version()
        );

        if rng.random_bool(0.25) {
            buffer.randomly_undo_redo(&mut rng);
            reference_string = buffer.text();
            log::info!(
                "buffer text {:?}, version: {:?}",
                buffer.text(),
                buffer.version()
            );
        }

        let range = buffer.random_byte_range(0, &mut rng);
        assert_eq!(
            buffer.text_summary_for_range::<TextSummary, _>(range.clone()),
            TextSummary::from(&reference_string[range])
        );

        buffer.check_invariants();

        if rng.random_bool(0.3) {
            buffer_versions.push((buffer.clone(), buffer.subscribe()));
        }
    }

    for (old_buffer, subscription) in buffer_versions {
        let edits = buffer
            .edits_since::<usize>(&old_buffer.version)
            .collect::<Vec<_>>();

        log::info!(
            "applying edits since version {:?} to old text: {:?}: {:?}",
            old_buffer.version(),
            old_buffer.text(),
            edits,
        );

        let mut text = old_buffer.visible_text.clone();
        for edit in edits {
            let new_text: String = buffer.text_for_range(edit.new.clone()).collect();
            text.replace(edit.new.start..edit.new.start + edit.old.len(), &new_text);
        }
        assert_eq!(text.to_string(), buffer.text());

        assert_eq!(
            buffer.rope_for_version(old_buffer.version()).to_string(),
            old_buffer.text()
        );

        for _ in 0..5 {
            let end_ix =
                old_buffer.clip_offset(rng.random_range(0..=old_buffer.len()), Bias::Right);
            let start_ix = old_buffer.clip_offset(rng.random_range(0..=end_ix), Bias::Left);
            let range = old_buffer.anchor_before(start_ix)..old_buffer.anchor_after(end_ix);
            let mut old_text = old_buffer.text_for_range(range.clone()).collect::<String>();
            let edits = buffer
                .edits_since_in_range::<usize>(&old_buffer.version, range.clone())
                .collect::<Vec<_>>();
            log::info!(
                "applying edits since version {:?} to old text in range {:?}: {:?}: {:?}",
                old_buffer.version(),
                start_ix..end_ix,
                old_text,
                edits,
            );

            let new_text = buffer.text_for_range(range).collect::<String>();
            for edit in edits {
                old_text.replace_range(
                    edit.new.start..edit.new.start + edit.old_len(),
                    &new_text[edit.new],
                );
            }
            assert_eq!(old_text, new_text);
        }

        assert_eq!(
            buffer.has_edits_since(&old_buffer.version),
            buffer
                .edits_since::<usize>(&old_buffer.version)
                .next()
                .is_some(),
        );

        let subscription_edits = subscription.consume();
        log::info!(
            "applying subscription edits since version {:?} to old text: {:?}: {:?}",
            old_buffer.version(),
            old_buffer.text(),
            subscription_edits,
        );

        let mut text = old_buffer.visible_text.clone();
        for edit in subscription_edits.into_inner() {
            let new_text: String = buffer.text_for_range(edit.new.clone()).collect();
            text.replace(edit.new.start..edit.new.start + edit.old.len(), &new_text);
        }
        assert_eq!(text.to_string(), buffer.text());
    }
}
