use super::*;

#[gpui::test(iterations = 100)]
async fn test_random_set_ranges(cx: &mut TestAppContext, mut rng: StdRng) {
    let base_text = "a\n".repeat(100);
    let buf = cx.update(|cx| cx.new(|cx| Buffer::local(base_text, cx)));
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    fn row_ranges(ranges: &Vec<Range<Point>>) -> Vec<Range<u32>> {
        ranges
            .iter()
            .map(|range| range.start.row..range.end.row)
            .collect()
    }

    for _ in 0..operations {
        let snapshot = buf.update(cx, |buf, _| buf.snapshot());
        let num_ranges = rng.random_range(0..=10);
        let max_row = snapshot.max_point().row;
        let mut ranges = (0..num_ranges)
            .map(|_| {
                let start = rng.random_range(0..max_row);
                let end = rng.random_range(start + 1..max_row + 1);
                Point::row_range(start..end)
            })
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| range.start);
        log::info!("Setting ranges: {:?}", row_ranges(&ranges));
        multibuffer.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::for_buffer(&buf, cx),
                buf.clone(),
                ranges.clone(),
                2,
                cx,
            )
        });

        let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
        let mut last_end = None;
        let mut seen_ranges = Vec::default();

        for info in snapshot.excerpts() {
            let buffer_snapshot = snapshot
                .buffer_for_id(info.context.start.buffer_id)
                .unwrap();
            let start = info.context.start.to_point(buffer_snapshot);
            let end = info.context.end.to_point(buffer_snapshot);
            seen_ranges.push(start..end);

            if let Some(last_end) = last_end.take() {
                assert!(
                    start > last_end,
                    "multibuffer has out-of-order ranges: {:?}; {:?} <= {:?}",
                    row_ranges(&seen_ranges),
                    start,
                    last_end
                )
            }

            ranges.retain(|range| range.start < start || range.end > end);

            last_end = Some(end)
        }

        assert!(
            ranges.is_empty(),
            "multibuffer {:?} did not include all ranges: {:?}",
            row_ranges(&seen_ranges),
            row_ranges(&ranges)
        );
    }
}
