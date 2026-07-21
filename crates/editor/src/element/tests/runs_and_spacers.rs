use super::*;

#[gpui::test]
fn test_split_runs_by_bg_segments(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let dx = |start: u32, end: u32| {
        DisplayPoint::new(DisplayRow(0), start)..DisplayPoint::new(DisplayRow(0), end)
    };

    let text_color = Hsla {
        h: 210.0,
        s: 0.1,
        l: 0.4,
        a: 1.0,
    };
    let bg_1 = Hsla {
        h: 30.0,
        s: 0.6,
        l: 0.8,
        a: 1.0,
    };
    let bg_2 = Hsla {
        h: 200.0,
        s: 0.6,
        l: 0.2,
        a: 1.0,
    };
    let min_contrast = 45.0;
    let adjusted_bg1 = ensure_minimum_contrast(text_color, bg_1, min_contrast);
    let adjusted_bg2 = ensure_minimum_contrast(text_color, bg_2, min_contrast);

    // Case A: single run; disjoint segments inside the run
    {
        let runs = vec![generate_test_run(20, text_color)];
        let segs = vec![(dx(5, 10), bg_1), (dx(12, 16), bg_2)];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
        // Expected slices: [0,5) [5,10) [10,12) [12,16) [16,20)
        assert_eq!(
            out.iter().map(|r| r.len).collect::<Vec<_>>(),
            vec![5, 5, 2, 4, 4]
        );
        assert_eq!(out[0].color, text_color);
        assert_eq!(out[1].color, adjusted_bg1);
        assert_eq!(out[2].color, text_color);
        assert_eq!(out[3].color, adjusted_bg2);
        assert_eq!(out[4].color, text_color);
    }

    // Case B: multiple runs; segment extends to end of line (u32::MAX)
    {
        let runs = vec![
            generate_test_run(8, text_color),
            generate_test_run(7, text_color),
        ];
        let segs = vec![(dx(6, u32::MAX), bg_1)];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
        // Expected slices across runs: [0,6) [6,8) | [0,7)
        assert_eq!(out.iter().map(|r| r.len).collect::<Vec<_>>(), vec![6, 2, 7]);
        assert_eq!(out[0].color, text_color);
        assert_eq!(out[1].color, adjusted_bg1);
        assert_eq!(out[2].color, adjusted_bg1);
    }

    // Case C: multi-byte characters
    {
        // for text: "Hello 🌍 世界!"
        let runs = vec![
            generate_test_run(5, text_color), // "Hello"
            generate_test_run(6, text_color), // " 🌍 "
            generate_test_run(6, text_color), // "世界"
            generate_test_run(1, text_color), // "!"
        ];
        // selecting "🌍 世"
        let segs = vec![(dx(6, 14), bg_1)];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
        // "Hello" | " " | "🌍 " | "世" | "界" | "!"
        assert_eq!(
            out.iter().map(|r| r.len).collect::<Vec<_>>(),
            vec![5, 1, 5, 3, 3, 1]
        );
        assert_eq!(out[0].color, text_color); // "Hello"
        assert_eq!(out[2].color, adjusted_bg1); // "🌍 "
        assert_eq!(out[3].color, adjusted_bg1); // "世"
        assert_eq!(out[4].color, text_color); // "界"
        assert_eq!(out[5].color, text_color); // "!"
    }

    // Case D: split multiple consecutive text runs with segments
    {
        let segs = vec![
            (dx(2, 4), bg_1),   // selecting "cd"
            (dx(4, 8), bg_2),   // selecting "efgh"
            (dx(9, 11), bg_1),  // selecting "jk"
            (dx(12, 16), bg_2), // selecting "mnop"
            (dx(18, 19), bg_1), // selecting "s"
        ];

        // for text: "abcdef"
        let runs = vec![
            generate_test_run(2, text_color), // ab
            generate_test_run(4, text_color), // cdef
        ];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 0);
        // new splits "ab", "cd", "ef"
        assert_eq!(out.iter().map(|r| r.len).collect::<Vec<_>>(), vec![2, 2, 2]);
        assert_eq!(out[0].color, text_color);
        assert_eq!(out[1].color, adjusted_bg1);
        assert_eq!(out[2].color, adjusted_bg2);

        // for text: "ghijklmn"
        let runs = vec![
            generate_test_run(3, text_color), // ghi
            generate_test_run(2, text_color), // jk
            generate_test_run(3, text_color), // lmn
        ];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 6); // 2 + 4 from first run
        // new splits "gh", "i", "jk", "l", "mn"
        assert_eq!(
            out.iter().map(|r| r.len).collect::<Vec<_>>(),
            vec![2, 1, 2, 1, 2]
        );
        assert_eq!(out[0].color, adjusted_bg2);
        assert_eq!(out[1].color, text_color);
        assert_eq!(out[2].color, adjusted_bg1);
        assert_eq!(out[3].color, text_color);
        assert_eq!(out[4].color, adjusted_bg2);

        // for text: "opqrs"
        let runs = vec![
            generate_test_run(1, text_color), // o
            generate_test_run(4, text_color), // pqrs
        ];
        let out = LineWithInvisibles::split_runs_by_bg_segments(&runs, &segs, min_contrast, 14); // 6 + 3 + 2 + 3 from first two runs
        // new splits "o", "p", "qr", "s"
        assert_eq!(
            out.iter().map(|r| r.len).collect::<Vec<_>>(),
            vec![1, 1, 2, 1]
        );
        assert_eq!(out[0].color, adjusted_bg2);
        assert_eq!(out[1].color, adjusted_bg2);
        assert_eq!(out[2].color, text_color);
        assert_eq!(out[3].color, adjusted_bg1);
    }
}

#[test]
fn test_spacer_pattern_period() {
    // line height is smaller than target height, so we just return half the line height
    assert_eq!(EditorElement::spacer_pattern_period(10.0, 20.0), 5.0);

    // line height is exactly half the target height, perfect match
    assert_eq!(EditorElement::spacer_pattern_period(20.0, 10.0), 10.0);

    // line height is close to half the target height
    assert_eq!(EditorElement::spacer_pattern_period(20.0, 9.0), 10.0);

    // line height is close to 1/4 the target height
    assert_eq!(EditorElement::spacer_pattern_period(20.0, 4.8), 5.0);
}

#[gpui::test(iterations = 100)]
fn test_random_spacer_pattern_period(mut rng: StdRng) {
    let line_height = rng.next_u32() as f32;
    let target_height = rng.next_u32() as f32;

    let result = EditorElement::spacer_pattern_period(line_height, target_height);

    let k = line_height / result;
    assert!(k - k.round() < 0.0000001); // approximately integer
    assert!((k.round() as u32).is_multiple_of(2));
}

#[test]
fn test_calculate_wrap_width() {
    let editor_width = px(800.0);
    let em_width = px(8.0);

    assert_eq!(
        calculate_wrap_width(SoftWrap::GitDiff, editor_width, em_width),
        None,
    );

    assert_eq!(
        calculate_wrap_width(SoftWrap::None, editor_width, em_width),
        Some(px((MAX_LINE_LEN as f32 / 2.0 * 8.0).ceil())),
    );

    assert_eq!(
        calculate_wrap_width(SoftWrap::EditorWidth, editor_width, em_width),
        Some(px(800.0)),
    );

    assert_eq!(
        calculate_wrap_width(SoftWrap::Bounded(72), editor_width, em_width),
        Some(px((72.0 * 8.0_f32).ceil())),
    );
    assert_eq!(
        calculate_wrap_width(SoftWrap::Bounded(200), px(400.0), em_width),
        Some(px(400.0)),
    );
}
