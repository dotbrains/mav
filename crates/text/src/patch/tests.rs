use rand::prelude::*;
use std::env;

use super::*;

#[gpui::test]
fn test_one_disjoint_edit() {
    assert_patch_composition(
        Patch(vec![Edit {
            old: 1..3,
            new: 1..4,
        }]),
        Patch(vec![Edit {
            old: 0..0,
            new: 0..4,
        }]),
        Patch(vec![
            Edit {
                old: 0..0,
                new: 0..4,
            },
            Edit {
                old: 1..3,
                new: 5..8,
            },
        ]),
    );

    assert_patch_composition(
        Patch(vec![Edit {
            old: 1..3,
            new: 1..4,
        }]),
        Patch(vec![Edit {
            old: 5..9,
            new: 5..7,
        }]),
        Patch(vec![
            Edit {
                old: 1..3,
                new: 1..4,
            },
            Edit {
                old: 4..8,
                new: 5..7,
            },
        ]),
    );
}

#[gpui::test]
fn test_one_overlapping_edit() {
    assert_patch_composition(
        Patch(vec![Edit {
            old: 1..3,
            new: 1..4,
        }]),
        Patch(vec![Edit {
            old: 3..5,
            new: 3..6,
        }]),
        Patch(vec![Edit {
            old: 1..4,
            new: 1..6,
        }]),
    );
}

#[gpui::test]
fn test_two_disjoint_and_overlapping() {
    assert_patch_composition(
        Patch(vec![
            Edit {
                old: 1..3,
                new: 1..4,
            },
            Edit {
                old: 8..12,
                new: 9..11,
            },
        ]),
        Patch(vec![
            Edit {
                old: 0..0,
                new: 0..4,
            },
            Edit {
                old: 3..10,
                new: 7..9,
            },
        ]),
        Patch(vec![
            Edit {
                old: 0..0,
                new: 0..4,
            },
            Edit {
                old: 1..12,
                new: 5..10,
            },
        ]),
    );
}

#[gpui::test]
fn test_two_new_edits_overlapping_one_old_edit() {
    assert_patch_composition(
        Patch(vec![Edit {
            old: 0..0,
            new: 0..3,
        }]),
        Patch(vec![
            Edit {
                old: 0..0,
                new: 0..1,
            },
            Edit {
                old: 1..2,
                new: 2..2,
            },
        ]),
        Patch(vec![Edit {
            old: 0..0,
            new: 0..3,
        }]),
    );

    assert_patch_composition(
        Patch(vec![Edit {
            old: 2..3,
            new: 2..4,
        }]),
        Patch(vec![
            Edit {
                old: 0..2,
                new: 0..1,
            },
            Edit {
                old: 3..3,
                new: 2..5,
            },
        ]),
        Patch(vec![Edit {
            old: 0..3,
            new: 0..6,
        }]),
    );

    assert_patch_composition(
        Patch(vec![Edit {
            old: 0..0,
            new: 0..2,
        }]),
        Patch(vec![
            Edit {
                old: 0..0,
                new: 0..2,
            },
            Edit {
                old: 2..5,
                new: 4..4,
            },
        ]),
        Patch(vec![Edit {
            old: 0..3,
            new: 0..4,
        }]),
    );
}

#[gpui::test]
fn test_two_new_edits_touching_one_old_edit() {
    assert_patch_composition(
        Patch(vec![
            Edit {
                old: 2..3,
                new: 2..4,
            },
            Edit {
                old: 7..7,
                new: 8..11,
            },
        ]),
        Patch(vec![
            Edit {
                old: 2..3,
                new: 2..2,
            },
            Edit {
                old: 4..4,
                new: 3..4,
            },
        ]),
        Patch(vec![
            Edit {
                old: 2..3,
                new: 2..4,
            },
            Edit {
                old: 7..7,
                new: 8..11,
            },
        ]),
    );
}

#[gpui::test]
fn test_old_to_new() {
    let patch = Patch(vec![
        Edit {
            old: 2..4,
            new: 2..4,
        },
        Edit {
            old: 7..8,
            new: 7..11,
        },
    ]);
    assert_eq!(patch.old_to_new(0), 0);
    assert_eq!(patch.old_to_new(1), 1);
    assert_eq!(patch.old_to_new(2), 2);
    assert_eq!(patch.old_to_new(3), 2);
    assert_eq!(patch.old_to_new(4), 4);
    assert_eq!(patch.old_to_new(5), 5);
    assert_eq!(patch.old_to_new(6), 6);
    assert_eq!(patch.old_to_new(7), 7);
    assert_eq!(patch.old_to_new(8), 11);
    assert_eq!(patch.old_to_new(9), 12);
}

#[gpui::test(iterations = 100)]
fn test_random_patch_compositions(mut rng: StdRng) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(20);

    let initial_chars = (0..rng.random_range(0..=100))
        .map(|_| rng.random_range(b'a'..=b'z') as char)
        .collect::<Vec<_>>();
    log::info!("initial chars: {:?}", initial_chars);

    let mut patches = Vec::new();
    let mut expected_chars = initial_chars.clone();
    for i in 0..2 {
        log::info!("patch {}:", i);

        let mut delta = 0i32;
        let mut last_edit_end = 0;
        let mut edits = Vec::new();

        for _ in 0..operations {
            if last_edit_end >= expected_chars.len() {
                break;
            }

            let end = rng.random_range(last_edit_end..=expected_chars.len());
            let start = rng.random_range(last_edit_end..=end);
            let old_len = end - start;

            let mut new_len = rng.random_range(0..=3);
            if start == end && new_len == 0 {
                new_len += 1;
            }

            last_edit_end = start + new_len + 1;

            let new_chars = (0..new_len)
                .map(|_| rng.random_range(b'A'..=b'Z') as char)
                .collect::<Vec<_>>();
            log::info!(
                "  editing {:?}: {:?}",
                start..end,
                new_chars.iter().collect::<String>()
            );
            edits.push(Edit {
                old: (start as i32 - delta) as u32..(end as i32 - delta) as u32,
                new: start as u32..(start + new_len) as u32,
            });
            expected_chars.splice(start..end, new_chars);

            delta += new_len as i32 - old_len as i32;
        }

        patches.push(Patch(edits));
    }

    log::info!("old patch: {:?}", &patches[0]);
    log::info!("new patch: {:?}", &patches[1]);
    log::info!("initial chars: {:?}", initial_chars);
    log::info!("final chars: {:?}", expected_chars);

    let composed = patches[0].compose(&patches[1]);
    log::info!("composed patch: {:?}", &composed);

    let mut actual_chars = initial_chars;
    for edit in composed.0 {
        actual_chars.splice(
            edit.new.start as usize..edit.new.start as usize + edit.old.len(),
            expected_chars[edit.new.start as usize..edit.new.end as usize]
                .iter()
                .copied(),
        );
    }

    assert_eq!(actual_chars, expected_chars);
}

#[track_caller]
#[allow(clippy::almost_complete_range)]
fn assert_patch_composition(old: Patch<u32>, new: Patch<u32>, composed: Patch<u32>) {
    let original = ('a'..'z').collect::<Vec<_>>();
    let inserted = ('A'..'Z').collect::<Vec<_>>();

    let mut expected = original.clone();
    apply_patch(&mut expected, &old, &inserted);
    apply_patch(&mut expected, &new, &inserted);

    let mut actual = original;
    apply_patch(&mut actual, &composed, &expected);
    assert_eq!(
        actual.into_iter().collect::<String>(),
        expected.into_iter().collect::<String>(),
        "expected patch is incorrect"
    );

    assert_eq!(old.compose(&new), composed);
}

fn apply_patch(text: &mut Vec<char>, patch: &Patch<u32>, new_text: &[char]) {
    for edit in patch.0.iter().rev() {
        text.splice(
            edit.old.start as usize..edit.old.end as usize,
            new_text[edit.new.start as usize..edit.new.end as usize]
                .iter()
                .copied(),
        );
    }
}
