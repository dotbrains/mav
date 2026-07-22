use super::*;

#[test]
fn test_contiguous_ranges() {
    assert_eq!(
        contiguous_ranges([1, 2, 3, 5, 6, 9, 10, 11, 12].into_iter(), 100).collect::<Vec<_>>(),
        &[1..4, 5..7, 9..13]
    );

    // Respects the `max_len` parameter
    assert_eq!(
        contiguous_ranges(
            [2, 3, 4, 5, 6, 7, 8, 9, 23, 24, 25, 26, 30, 31].into_iter(),
            3
        )
        .collect::<Vec<_>>(),
        &[2..5, 5..8, 8..10, 23..26, 26..27, 30..32],
    );
}

#[gpui::test]
fn test_insertion_after_deletion(cx: &mut gpui::App) {
    let buffer = cx.new(|cx| Buffer::local("struct Foo {\n    \n}", cx));
    buffer.update(cx, |buffer, cx| {
        let mut anchor = buffer.anchor_after(17);
        buffer.edit([(12..18, "")], None, cx);
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.text(), "struct Foo {}");
        if !anchor.is_valid(&snapshot) {
            anchor = snapshot.anchor_after(snapshot.offset_for_anchor(&anchor));
        }
        buffer.edit([(anchor..anchor, "\n")], None, cx);
        buffer.edit([(anchor..anchor, "field1:")], None, cx);
        buffer.edit([(anchor..anchor, " i32,")], None, cx);
        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.text(), "struct Foo {\nfield1: i32,}");
    })
}

#[gpui::test(iterations = 500)]
fn test_trailing_whitespace_ranges(mut rng: StdRng) {
    // Generate a random multi-line string containing
    // some lines with trailing whitespace.
    let mut text = String::new();
    for _ in 0..rng.random_range(0..16) {
        for _ in 0..rng.random_range(0..36) {
            text.push(match rng.random_range(0..10) {
                0..=1 => ' ',
                3 => '\t',
                _ => rng.random_range('a'..='z'),
            });
        }
        text.push('\n');
    }

    match rng.random_range(0..10) {
        // sometimes remove the last newline
        0..=1 => drop(text.pop()), //

        // sometimes add extra newlines
        2..=3 => text.push_str(&"\n".repeat(rng.random_range(1..5))),
        _ => {}
    }

    let rope = Rope::from(text.as_str());
    let actual_ranges = trailing_whitespace_ranges(&rope);
    let expected_ranges = TRAILING_WHITESPACE_REGEX
        .find_iter(&text)
        .map(|m| m.range())
        .collect::<Vec<_>>();
    assert_eq!(
        actual_ranges,
        expected_ranges,
        "wrong ranges for text lines:\n{:?}",
        text.split('\n').collect::<Vec<_>>()
    );
}

#[gpui::test]
fn test_words_in_range(cx: &mut gpui::App) {
    init_settings(cx, |_| {});

    // The first line are words excluded from the results with heuristics, we do not expect them in the test assertions.
    let contents = r#"
0_isize 123 3.4 4  
let word=öäpple.bar你 Öäpple word2-öÄpPlE-Pizza-word ÖÄPPLE word
    "#;

    let buffer = cx.new(|cx| {
        let buffer = Buffer::local(contents, cx).with_language(rust_lang(), cx);
        assert_eq!(buffer.text(), contents);
        buffer.check_invariants();
        buffer
    });

    buffer.update(cx, |buffer, _| {
        let snapshot = buffer.snapshot();
        assert_eq!(
            BTreeSet::from_iter(["Pizza".to_string()]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some("piz"),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::from_iter([
                "öäpple".to_string(),
                "Öäpple".to_string(),
                "öÄpPlE".to_string(),
                "ÖÄPPLE".to_string(),
            ]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some("öp"),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::from_iter([
                "öÄpPlE".to_string(),
                "Öäpple".to_string(),
                "ÖÄPPLE".to_string(),
                "öäpple".to_string(),
            ]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some("öÄ"),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::default(),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some("öÄ好"),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::from_iter(["bar你".to_string(),]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some("你"),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::default(),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: Some(""),
                    skip_digits: true,
                    range: 0..snapshot.len(),
                },)
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::from_iter([
                "bar你".to_string(),
                "öÄpPlE".to_string(),
                "Öäpple".to_string(),
                "ÖÄPPLE".to_string(),
                "öäpple".to_string(),
                "let".to_string(),
                "Pizza".to_string(),
                "word".to_string(),
                "word2".to_string(),
            ]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: None,
                    skip_digits: true,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            BTreeSet::from_iter([
                "0_isize".to_string(),
                "123".to_string(),
                "3".to_string(),
                "4".to_string(),
                "bar你".to_string(),
                "öÄpPlE".to_string(),
                "Öäpple".to_string(),
                "ÖÄPPLE".to_string(),
                "öäpple".to_string(),
                "let".to_string(),
                "Pizza".to_string(),
                "word".to_string(),
                "word2".to_string(),
            ]),
            snapshot
                .words_in_range(WordsQuery {
                    fuzzy_contents: None,
                    skip_digits: false,
                    range: 0..snapshot.len(),
                })
                .into_keys()
                .collect::<BTreeSet<_>>()
        );
    });
}
