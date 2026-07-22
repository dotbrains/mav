use super::*;

impl CodegenAlternative {
    pub(super) fn apply_edits(
        &mut self,
        edits: impl IntoIterator<Item = (Range<Anchor>, String)>,
        cx: &mut Context<CodegenAlternative>,
    ) {
        let transaction = self.buffer.update(cx, |buffer, cx| {
            // Avoid grouping agent edits with user edits.
            buffer.finalize_last_transaction(cx);
            buffer.start_transaction(cx);
            buffer.edit(edits, None, cx);
            buffer.end_transaction_with_source(BufferEditSource::Agent, cx)
        });

        if let Some(transaction) = transaction {
            if let Some(first_transaction) = self.transformation_transaction_id {
                // Group all agent edits into the first transaction.
                self.buffer.update(cx, |buffer, cx| {
                    buffer.merge_transactions(transaction, first_transaction, cx)
                });
            } else {
                self.transformation_transaction_id = Some(transaction);
                self.buffer
                    .update(cx, |buffer, cx| buffer.finalize_last_transaction(cx));
            }
        }
    }

    pub(super) fn reapply_line_based_diff(
        &mut self,
        line_operations: impl IntoIterator<Item = LineOperation>,
        cx: &mut Context<Self>,
    ) {
        let old_snapshot = self.snapshot.clone();
        let old_range = self.range.to_point(&old_snapshot);
        let new_snapshot = self.buffer.read(cx).snapshot(cx);
        let new_range = self.range.to_point(&new_snapshot);

        let mut old_row = old_range.start.row;
        let mut new_row = new_range.start.row;

        self.diff.deleted_row_ranges.clear();
        self.diff.inserted_row_ranges.clear();
        for operation in line_operations {
            match operation {
                LineOperation::Keep { lines } => {
                    old_row += lines;
                    new_row += lines;
                }
                LineOperation::Delete { lines } => {
                    let old_end_row = old_row + lines - 1;
                    let new_row = new_snapshot.anchor_before(Point::new(new_row, 0));

                    if let Some((_, last_deleted_row_range)) =
                        self.diff.deleted_row_ranges.last_mut()
                    {
                        if *last_deleted_row_range.end() + 1 == old_row {
                            *last_deleted_row_range = *last_deleted_row_range.start()..=old_end_row;
                        } else {
                            self.diff
                                .deleted_row_ranges
                                .push((new_row, old_row..=old_end_row));
                        }
                    } else {
                        self.diff
                            .deleted_row_ranges
                            .push((new_row, old_row..=old_end_row));
                    }

                    old_row += lines;
                }
                LineOperation::Insert { lines } => {
                    let new_end_row = new_row + lines - 1;
                    let start = new_snapshot.anchor_before(Point::new(new_row, 0));
                    let end = new_snapshot.anchor_before(Point::new(
                        new_end_row,
                        new_snapshot.line_len(MultiBufferRow(new_end_row)),
                    ));
                    self.diff.inserted_row_ranges.push(start..end);
                    new_row += lines;
                }
            }

            cx.notify();
        }
    }

    pub(super) fn reapply_batch_diff(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let old_snapshot = self.snapshot.clone();
        let old_range = self.range.to_point(&old_snapshot);
        let new_snapshot = self.buffer.read(cx).snapshot(cx);
        let new_range = self.range.to_point(&new_snapshot);

        cx.spawn(async move |codegen, cx| {
            let (deleted_row_ranges, inserted_row_ranges) = cx
                .background_spawn(async move {
                    let old_text = old_snapshot
                        .text_for_range(
                            Point::new(old_range.start.row, 0)
                                ..Point::new(
                                    old_range.end.row,
                                    old_snapshot.line_len(MultiBufferRow(old_range.end.row)),
                                ),
                        )
                        .collect::<String>();
                    let new_text = new_snapshot
                        .text_for_range(
                            Point::new(new_range.start.row, 0)
                                ..Point::new(
                                    new_range.end.row,
                                    new_snapshot.line_len(MultiBufferRow(new_range.end.row)),
                                ),
                        )
                        .collect::<String>();

                    let old_start_row = old_range.start.row;
                    let new_start_row = new_range.start.row;
                    let mut deleted_row_ranges: Vec<(Anchor, RangeInclusive<u32>)> = Vec::new();
                    let mut inserted_row_ranges = Vec::new();
                    for (old_rows, new_rows) in line_diff(&old_text, &new_text) {
                        let old_rows = old_start_row + old_rows.start..old_start_row + old_rows.end;
                        let new_rows = new_start_row + new_rows.start..new_start_row + new_rows.end;
                        if !old_rows.is_empty() {
                            deleted_row_ranges.push((
                                new_snapshot.anchor_before(Point::new(new_rows.start, 0)),
                                old_rows.start..=old_rows.end - 1,
                            ));
                        }
                        if !new_rows.is_empty() {
                            let start = new_snapshot.anchor_before(Point::new(new_rows.start, 0));
                            let new_end_row = new_rows.end - 1;
                            let end = new_snapshot.anchor_before(Point::new(
                                new_end_row,
                                new_snapshot.line_len(MultiBufferRow(new_end_row)),
                            ));
                            inserted_row_ranges.push(start..end);
                        }
                    }
                    (deleted_row_ranges, inserted_row_ranges)
                })
                .await;

            codegen
                .update(cx, |codegen, cx| {
                    codegen.diff.deleted_row_ranges = deleted_row_ranges;
                    codegen.diff.inserted_row_ranges = inserted_row_ranges;
                    cx.notify();
                })
                .ok();
        })
    }
}
