use super::*;

impl Buffer {
    #[track_caller]
    pub fn edit_via_marked_text(&mut self, marked_string: &str) {
        let edits = self.edits_for_marked_text(marked_string);
        self.edit(edits);
    }

    #[track_caller]
    pub fn edits_for_marked_text(&self, marked_string: &str) -> Vec<(Range<usize>, String)> {
        let old_text = self.text();
        let (new_text, mut ranges) = util::test::marked_text_ranges(marked_string, false);
        if ranges.is_empty() {
            ranges.push(0..new_text.len());
        }

        assert_eq!(
            old_text[..ranges[0].start],
            new_text[..ranges[0].start],
            "invalid edit"
        );

        let mut delta = 0;
        let mut edits = Vec::new();
        let mut ranges = ranges.into_iter().peekable();

        while let Some(inserted_range) = ranges.next() {
            let new_start = inserted_range.start;
            let old_start = (new_start as isize - delta) as usize;

            let following_text = if let Some(next_range) = ranges.peek() {
                &new_text[inserted_range.end..next_range.start]
            } else {
                &new_text[inserted_range.end..]
            };

            let inserted_len = inserted_range.len();
            let deleted_len = old_text[old_start..]
                .find(following_text)
                .expect("invalid edit");

            let old_range = old_start..old_start + deleted_len;
            edits.push((old_range, new_text[inserted_range].to_string()));
            delta += inserted_len as isize - deleted_len as isize;
        }

        assert_eq!(
            old_text.len() as isize + delta,
            new_text.len() as isize,
            "invalid edit"
        );

        edits
    }

    pub fn check_invariants(&self) {
        // Ensure every fragment is ordered by locator in the fragment tree and corresponds
        // to an insertion fragment in the insertions tree.
        let mut prev_fragment_id = Locator::min();
        for fragment in self.snapshot.fragments.items(&None) {
            assert!(fragment.id > prev_fragment_id);
            prev_fragment_id = fragment.id.clone();

            let insertion_fragment = self
                .snapshot
                .insertions
                .get(
                    &InsertionFragmentKey {
                        timestamp: fragment.timestamp,
                        split_offset: fragment.insertion_offset,
                    },
                    (),
                )
                .unwrap();
            assert_eq!(
                insertion_fragment.fragment_id, fragment.id,
                "fragment: {:?}\ninsertion: {:?}",
                fragment, insertion_fragment
            );
        }

        let mut cursor = self.snapshot.fragments.cursor::<Option<&Locator>>(&None);
        for insertion_fragment in self.snapshot.insertions.cursor::<()>(()) {
            cursor.seek(&Some(&insertion_fragment.fragment_id), Bias::Left);
            let fragment = cursor.item().unwrap();
            assert_eq!(insertion_fragment.fragment_id, fragment.id);
            assert_eq!(insertion_fragment.split_offset, fragment.insertion_offset);
        }

        let fragment_summary = self.snapshot.fragments.summary();
        assert_eq!(
            fragment_summary.text.visible,
            self.snapshot.visible_text.len()
        );
        assert_eq!(
            fragment_summary.text.deleted,
            self.snapshot.deleted_text.len()
        );

        assert!(!self.text().contains("\r\n"));
    }

    pub fn random_byte_range(&self, start_offset: usize, rng: &mut impl rand::Rng) -> Range<usize> {
        let end = self.clip_offset(rng.random_range(start_offset..=self.len()), Bias::Right);
        let start = self.clip_offset(rng.random_range(start_offset..=end), Bias::Right);
        start..end
    }

    pub fn get_random_edits<T>(
        &self,
        rng: &mut T,
        edit_count: usize,
    ) -> Vec<(Range<usize>, Arc<str>)>
    where
        T: rand::Rng,
    {
        let mut edits: Vec<(Range<usize>, Arc<str>)> = Vec::new();
        let mut last_end = None;
        for _ in 0..edit_count {
            if last_end.is_some_and(|last_end| last_end >= self.len()) {
                break;
            }
            let new_start = last_end.map_or(0, |last_end| last_end + 1);
            let range = self.random_byte_range(new_start, rng);
            last_end = Some(range.end);

            let new_text_len = rng.random_range(0..10);
            let new_text: String = RandomCharIter::new(&mut *rng).take(new_text_len).collect();

            edits.push((range, new_text.into()));
        }
        edits
    }

    pub fn randomly_edit<T>(
        &mut self,
        rng: &mut T,
        edit_count: usize,
    ) -> (Vec<(Range<usize>, Arc<str>)>, Operation)
    where
        T: rand::Rng,
    {
        let mut edits = self.get_random_edits(rng, edit_count);
        log::info!("mutating buffer {:?} with {:?}", self.replica_id, edits);

        let op = self.edit(edits.iter().cloned());
        if let Operation::Edit(edit) = &op {
            assert_eq!(edits.len(), edit.new_text.len());
            for (edit, new_text) in edits.iter_mut().zip(&edit.new_text) {
                edit.1 = new_text.clone();
            }
        } else {
            unreachable!()
        }

        (edits, op)
    }

    pub fn randomly_undo_redo(&mut self, rng: &mut impl rand::Rng) -> Vec<Operation> {
        use rand::prelude::*;

        let mut ops = Vec::new();
        for _ in 0..rng.random_range(1..=5) {
            if let Some(entry) = self.history.undo_stack.choose(rng) {
                let transaction = entry.transaction.clone();
                log::info!(
                    "undoing buffer {:?} transaction {:?}",
                    self.replica_id,
                    transaction
                );
                ops.push(self.undo_or_redo(transaction));
            }
        }
        ops
    }
}
