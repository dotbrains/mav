use super::*;

impl BufferSnapshot {
    /// Returns the line indents in the given row range, exclusive of end row, in reversed order.
    pub fn reversed_line_indents_in_row_range(
        &self,
        row_range: Range<u32>,
    ) -> impl Iterator<Item = (u32, LineIndent)> + '_ {
        let start = Point::new(row_range.start, 0).to_offset(self);

        let end_point;
        let end;
        if row_range.end > row_range.start {
            end_point = Point::new(row_range.end - 1, self.line_len(row_range.end - 1));
            end = end_point.to_offset(self);
        } else {
            end_point = Point::new(row_range.start, 0);
            end = start;
        };

        let mut chunks = self.as_rope().chunks_in_range(start..end);
        // Move the cursor to the start of the last line if it's not empty.
        chunks.seek(end);
        if end_point.column > 0 {
            chunks.prev_line();
        }

        let mut row = end_point.row;
        let mut done = false;
        std::iter::from_fn(move || {
            if done {
                None
            } else {
                let initial_offset = chunks.offset();
                let indent = (row, LineIndent::from_chunks(&mut chunks));
                if chunks.offset() > initial_offset {
                    chunks.prev_line();
                }
                done = !chunks.prev_line();
                if !done {
                    row -= 1;
                }

                Some(indent)
            }
        })
    }

    pub fn line_indent_for_row(&self, row: u32) -> LineIndent {
        LineIndent::from_iter(self.chars_at(Point::new(row, 0)))
    }

    pub fn is_line_blank(&self, row: u32) -> bool {
        self.text_for_range(Point::new(row, 0)..Point::new(row, self.line_len(row)))
            .all(|chunk| chunk.matches(|c: char| !c.is_whitespace()).next().is_none())
    }

    pub fn text_summary_for_range<D, O: ToOffset>(&self, range: Range<O>) -> D
    where
        D: TextDimension,
    {
        self.visible_text
            .cursor(range.start.to_offset(self))
            .summary(range.end.to_offset(self))
    }

    pub fn summaries_for_anchors<'a, D, A>(&'a self, anchors: A) -> impl 'a + Iterator<Item = D>
    where
        D: 'a + TextDimension,
        A: 'a + IntoIterator<Item = Anchor>,
    {
        let anchors = anchors.into_iter();
        self.summaries_for_anchors_with_payload::<D, _, ()>(anchors.map(|a| (a, ())))
            .map(|d| d.0)
    }

    pub fn summaries_for_anchors_with_payload<'a, D, A, T>(
        &'a self,
        anchors: A,
    ) -> impl 'a + Iterator<Item = (D, T)>
    where
        D: 'a + TextDimension,
        A: 'a + IntoIterator<Item = (Anchor, T)>,
    {
        let anchors = anchors.into_iter();
        let mut fragment_cursor = self
            .fragments
            .cursor::<Dimensions<Option<&Locator>, usize>>(&None);
        let mut text_cursor = self.visible_text.cursor(0);
        let mut position = D::zero(());

        anchors.map(move |(anchor, payload)| {
            if anchor.is_min() {
                return (D::zero(()), payload);
            } else if anchor.is_max() {
                return (D::from_text_summary(&self.visible_text.summary()), payload);
            }

            let Some(insertion) = self.try_find_fragment(&anchor) else {
                panic!(
                    "invalid insertion for buffer {}@{:?} with anchor {:?}",
                    self.remote_id(),
                    self.version,
                    anchor
                );
            };
            // TODO verbose debug because we are seeing is_max return false unexpectedly,
            // remove this once that is understood and fixed
            assert_eq!(
                insertion.timestamp,
                anchor.timestamp(),
                "invalid insertion for buffer {}@{:?}. anchor: {:?}, {:?}, {:?}, {:?}, {:?}. timestamp: {:?}, offset: {:?}, bias: {:?}",
                self.remote_id(),
                self.version,
                anchor.timestamp_replica_id,
                anchor.timestamp_value,
                anchor.offset,
                anchor.bias,
                anchor.buffer_id,
                anchor.timestamp() == clock::Lamport::MAX,
                anchor.offset == u32::MAX,
                anchor.bias == Bias::Right,
            );

            fragment_cursor.seek_forward(&Some(&insertion.fragment_id), Bias::Left);
            let fragment = fragment_cursor.item().unwrap();
            let mut fragment_offset = fragment_cursor.start().1;
            if fragment.visible {
                fragment_offset += (anchor.offset - insertion.split_offset) as usize;
            }

            position.add_assign(&text_cursor.summary(fragment_offset));
            (position, payload)
        })
    }

    pub fn summary_for_anchor<D>(&self, anchor: &Anchor) -> D
    where
        D: TextDimension,
    {
        self.text_summary_for_range(0..self.offset_for_anchor(anchor))
    }

    pub fn offset_for_anchor(&self, anchor: &Anchor) -> usize {
        if anchor.is_min() {
            0
        } else if anchor.is_max() {
            self.visible_text.len()
        } else {
            debug_assert_eq!(anchor.buffer_id, self.remote_id);
            debug_assert!(
                self.version.observed(anchor.timestamp()),
                "Anchor timestamp {:?} not observed by buffer {:?}",
                anchor.timestamp(),
                self.version
            );
            let item = self.try_find_fragment(anchor);
            let Some(insertion) =
                item.filter(|insertion| insertion.timestamp == anchor.timestamp())
            else {
                self.panic_bad_anchor(anchor);
            };

            let (start, _, item) = self
                .fragments
                .find::<Dimensions<Option<&Locator>, usize>, _>(
                    &None,
                    &Some(&insertion.fragment_id),
                    Bias::Left,
                );
            let fragment = item.unwrap();
            let mut fragment_offset = start.1;
            if fragment.visible {
                fragment_offset += (anchor.offset - insertion.split_offset) as usize;
            }
            fragment_offset
        }
    }

    #[cold]
    fn panic_bad_anchor(&self, anchor: &Anchor) -> ! {
        if anchor.buffer_id != self.remote_id {
            panic!(
                "invalid anchor - buffer id does not match: anchor {anchor:?}; buffer id: {}, version: {:?}",
                self.remote_id, self.version
            );
        } else if !self.version.observed(anchor.timestamp()) {
            panic!(
                "invalid anchor - snapshot has not observed lamport: {:?}; version: {:?}",
                anchor, self.version
            );
        } else {
            panic!(
                "invalid anchor {:?}. buffer id: {}, version: {:?}",
                anchor, self.remote_id, self.version
            );
        }
    }

    pub(crate) fn fragment_id_for_anchor(&self, anchor: &Anchor) -> &Locator {
        self.try_fragment_id_for_anchor(anchor)
            .unwrap_or_else(|| self.panic_bad_anchor(anchor))
    }

    pub(crate) fn try_fragment_id_for_anchor(&self, anchor: &Anchor) -> Option<&Locator> {
        if anchor.is_min() {
            Some(Locator::min_ref())
        } else if anchor.is_max() {
            Some(Locator::max_ref())
        } else {
            let item = self.try_find_fragment(anchor);
            item.filter(|insertion| {
                !cfg!(debug_assertions) || insertion.timestamp == anchor.timestamp()
            })
            .map(|insertion| &insertion.fragment_id)
        }
    }

    fn try_find_fragment(&self, anchor: &Anchor) -> Option<&InsertionFragment> {
        let anchor_key = InsertionFragmentKey {
            timestamp: anchor.timestamp(),
            split_offset: anchor.offset,
        };
        match self.insertions.find_with_prev::<InsertionFragmentKey, _>(
            (),
            &anchor_key,
            anchor.bias,
        ) {
            (_, _, Some((prev, insertion))) => {
                let comparison = sum_tree::KeyedItem::key(insertion).cmp(&anchor_key);
                if comparison == Ordering::Greater
                    || (anchor.bias == Bias::Left
                        && comparison == Ordering::Equal
                        && anchor.offset > 0)
                {
                    prev
                } else {
                    Some(insertion)
                }
            }
            _ => self.insertions.last(),
        }
    }

    /// Returns an anchor range for the given input position range that is anchored to the text in the range.
    pub fn anchor_range_inside<T: ToOffset>(&self, position: Range<T>) -> Range<Anchor> {
        self.anchor_after(position.start)..self.anchor_before(position.end)
    }

    /// Returns an anchor range for the given input position range that is anchored to the text before and after.
    pub fn anchor_range_outside<T: ToOffset>(&self, position: Range<T>) -> Range<Anchor> {
        self.anchor_before(position.start)..self.anchor_after(position.end)
    }

    /// Returns an anchor for the given input position that is anchored to the text before the position.
    pub fn anchor_before<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Left)
    }

    /// Returns an anchor for the given input position that is anchored to the text after the position.
    pub fn anchor_after<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Right)
    }

    pub fn anchor_at<T: ToOffset>(&self, position: T, bias: Bias) -> Anchor {
        self.anchor_at_offset(position.to_offset(self), bias)
    }

    fn anchor_at_offset(&self, mut offset: usize, bias: Bias) -> Anchor {
        if bias == Bias::Left && offset == 0 {
            Anchor::min_for_buffer(self.remote_id)
        } else if bias == Bias::Right
            && ((!cfg!(debug_assertions) && offset >= self.len()) || offset == self.len())
        {
            Anchor::max_for_buffer(self.remote_id)
        } else {
            if !self
                .visible_text
                .assert_char_boundary::<{ cfg!(debug_assertions) }>(offset)
            {
                offset = match bias {
                    Bias::Left => self.visible_text.floor_char_boundary(offset),
                    Bias::Right => self.visible_text.ceil_char_boundary(offset),
                };
            }
            let (start, _, item) = self.fragments.find::<usize, _>(&None, &offset, bias);
            let Some(fragment) = item else {
                // We got a bad offset, likely out of bounds
                debug_panic!(
                    "Failed to find fragment at offset {} (len: {})",
                    offset,
                    self.len()
                );
                return Anchor::max_for_buffer(self.remote_id);
            };
            let overshoot = offset - start;
            Anchor::new(
                fragment.timestamp,
                fragment.insertion_offset + overshoot as u32,
                bias,
                self.remote_id,
            )
        }
    }

    pub fn can_resolve(&self, anchor: &Anchor) -> bool {
        anchor.is_min()
            || anchor.is_max()
            || (self.remote_id == anchor.buffer_id && self.version.observed(anchor.timestamp()))
    }

    pub fn clip_offset(&self, offset: usize, bias: Bias) -> usize {
        self.visible_text.clip_offset(offset, bias)
    }

    pub fn clip_point(&self, point: Point, bias: Bias) -> Point {
        self.visible_text.clip_point(point, bias)
    }

    pub fn clip_offset_utf16(&self, offset: OffsetUtf16, bias: Bias) -> OffsetUtf16 {
        self.visible_text.clip_offset_utf16(offset, bias)
    }

    pub fn clip_point_utf16(&self, point: Unclipped<PointUtf16>, bias: Bias) -> PointUtf16 {
        self.visible_text.clip_point_utf16(point, bias)
    }
}
