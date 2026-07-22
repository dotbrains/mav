use super::*;
use rand::{distr::StandardUniform, prelude::*};

#[test]
fn test_random() {
    let mut starting_seed = 0;
    if let Ok(value) = std::env::var("SEED") {
        starting_seed = value.parse().expect("invalid SEED variable");
    }
    let mut num_iterations = 100;
    if let Ok(value) = std::env::var("ITERATIONS") {
        num_iterations = value.parse().expect("invalid ITERATIONS variable");
    }
    let num_operations =
        std::env::var("OPERATIONS").map_or(5, |o| o.parse().expect("invalid OPERATIONS variable"));

    for seed in starting_seed..(starting_seed + num_iterations) {
        eprintln!("seed = {}", seed);
        let mut rng = StdRng::seed_from_u64(seed);

        let rng = &mut rng;
        let mut tree = SumTree::<u8>::default();
        let count = rng.random_range(0..10);
        if rng.random() {
            tree.extend(rng.sample_iter(StandardUniform).take(count), ());
        } else {
            let items = rng
                .sample_iter(StandardUniform)
                .take(count)
                .collect::<Vec<_>>();
            tree.par_extend(items, ());
        }

        for _ in 0..num_operations {
            let splice_end = rng.random_range(0..tree.extent::<Count>(()).0 + 1);
            let splice_start = rng.random_range(0..splice_end + 1);
            let count = rng.random_range(0..10);
            let tree_end = tree.extent::<Count>(());
            let new_items = rng
                .sample_iter(StandardUniform)
                .take(count)
                .collect::<Vec<u8>>();

            let mut reference_items = tree.items(());
            reference_items.splice(splice_start..splice_end, new_items.clone());

            tree = {
                let mut cursor = tree.cursor::<Count>(());
                let mut new_tree = cursor.slice(&Count(splice_start), Bias::Right);
                if rng.random() {
                    new_tree.extend(new_items, ());
                } else {
                    new_tree.par_extend(new_items, ());
                }
                cursor.seek(&Count(splice_end), Bias::Right);
                new_tree.append(cursor.slice(&tree_end, Bias::Right), ());
                new_tree
            };

            assert_eq!(tree.items(()), reference_items);
            assert_eq!(
                tree.iter().collect::<Vec<_>>(),
                tree.cursor::<()>(()).collect::<Vec<_>>()
            );

            log::info!("tree items: {:?}", tree.items(()));

            let mut filter_cursor =
                tree.filter::<_, Count>((), |summary: &IntegersSummary| summary.contains_even);
            let expected_filtered_items = tree
                .items(())
                .into_iter()
                .enumerate()
                .filter(|(_, item)| (item & 1) == 0)
                .collect::<Vec<_>>();

            let mut item_ix = if rng.random() {
                filter_cursor.next();
                0
            } else {
                filter_cursor.prev();
                expected_filtered_items.len().saturating_sub(1)
            };
            while item_ix < expected_filtered_items.len() {
                log::info!("filter_cursor, item_ix: {}", item_ix);
                let actual_item = filter_cursor.item().unwrap();
                let (reference_index, reference_item) = expected_filtered_items[item_ix];
                assert_eq!(actual_item, &reference_item);
                assert_eq!(filter_cursor.start().0, reference_index);
                log::info!("next");
                filter_cursor.next();
                item_ix += 1;

                while item_ix > 0 && rng.random_bool(0.2) {
                    log::info!("prev");
                    filter_cursor.prev();
                    item_ix -= 1;

                    if item_ix == 0 && rng.random_bool(0.2) {
                        filter_cursor.prev();
                        assert_eq!(filter_cursor.item(), None);
                        assert_eq!(filter_cursor.start().0, 0);
                        filter_cursor.next();
                    }
                }
            }
            assert_eq!(filter_cursor.item(), None);

            let mut before_start = false;
            let mut cursor = tree.cursor::<Count>(());
            let start_pos = rng.random_range(0..=reference_items.len());
            cursor.seek(&Count(start_pos), Bias::Right);
            let mut pos = rng.random_range(start_pos..=reference_items.len());
            cursor.seek_forward(&Count(pos), Bias::Right);

            for i in 0..10 {
                assert_eq!(cursor.start().0, pos);

                if pos > 0 {
                    assert_eq!(cursor.prev_item().unwrap(), &reference_items[pos - 1]);
                } else {
                    assert_eq!(cursor.prev_item(), None);
                }

                if pos < reference_items.len() && !before_start {
                    assert_eq!(cursor.item().unwrap(), &reference_items[pos]);
                } else {
                    assert_eq!(cursor.item(), None);
                }

                if before_start {
                    assert_eq!(cursor.next_item(), reference_items.first());
                } else if pos + 1 < reference_items.len() {
                    assert_eq!(cursor.next_item().unwrap(), &reference_items[pos + 1]);
                } else {
                    assert_eq!(cursor.next_item(), None);
                }

                if i < 5 {
                    cursor.next();
                    if pos < reference_items.len() {
                        pos += 1;
                        before_start = false;
                    }
                } else {
                    cursor.prev();
                    if pos == 0 {
                        before_start = true;
                    }
                    pos = pos.saturating_sub(1);
                }
            }
        }

        for _ in 0..10 {
            let end = rng.random_range(0..tree.extent::<Count>(()).0 + 1);
            let start = rng.random_range(0..end + 1);
            let start_bias = if rng.random() {
                Bias::Left
            } else {
                Bias::Right
            };
            let end_bias = if rng.random() {
                Bias::Left
            } else {
                Bias::Right
            };

            let mut cursor = tree.cursor::<Count>(());
            cursor.seek(&Count(start), start_bias);
            let slice = cursor.slice(&Count(end), end_bias);

            cursor.seek(&Count(start), start_bias);
            let summary = cursor.summary::<_, Sum>(&Count(end), end_bias);

            assert_eq!(summary.0, slice.summary().sum);
        }
    }
}
