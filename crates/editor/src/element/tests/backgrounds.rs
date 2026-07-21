use super::*;

#[gpui::test]
fn test_merge_overlapping_ranges() {
    let base_bg = Hsla::white();
    let color1 = Hsla {
        h: 0.0,
        s: 0.5,
        l: 0.5,
        a: 0.5,
    };
    let color2 = Hsla {
        h: 120.0,
        s: 0.5,
        l: 0.5,
        a: 0.5,
    };

    let display_point = |col| DisplayPoint::new(DisplayRow(0), col);
    let cols = |v: &Vec<(Range<DisplayPoint>, Hsla)>| -> Vec<(u32, u32)> {
        v.iter()
            .map(|(r, _)| (r.start.column(), r.end.column()))
            .collect()
    };

    // Test overlapping ranges blend colors
    let overlapping = vec![
        (display_point(5)..display_point(15), color1),
        (display_point(10)..display_point(20), color2),
    ];
    let result = EditorElement::merge_overlapping_ranges(overlapping, base_bg);
    assert_eq!(cols(&result), vec![(5, 10), (10, 15), (15, 20)]);

    // Test middle segment should have blended color
    let blended = Hsla::blend(Hsla::blend(base_bg, color1), color2);
    assert_eq!(result[1].1, blended);

    // Test adjacent same-color ranges merge
    let adjacent_same = vec![
        (display_point(5)..display_point(10), color1),
        (display_point(10)..display_point(15), color1),
    ];
    let result = EditorElement::merge_overlapping_ranges(adjacent_same, base_bg);
    assert_eq!(cols(&result), vec![(5, 15)]);

    // Test contained range splits
    let contained = vec![
        (display_point(5)..display_point(20), color1),
        (display_point(10)..display_point(15), color2),
    ];
    let result = EditorElement::merge_overlapping_ranges(contained, base_bg);
    assert_eq!(cols(&result), vec![(5, 10), (10, 15), (15, 20)]);

    // Test multiple overlaps split at every boundary
    let color3 = Hsla {
        h: 240.0,
        s: 0.5,
        l: 0.5,
        a: 0.5,
    };
    let complex = vec![
        (display_point(5)..display_point(12), color1),
        (display_point(8)..display_point(16), color2),
        (display_point(10)..display_point(14), color3),
    ];
    let result = EditorElement::merge_overlapping_ranges(complex, base_bg);
    assert_eq!(
        cols(&result),
        vec![(5, 8), (8, 10), (10, 12), (12, 14), (14, 16)]
    );
}

#[gpui::test]
fn test_bg_segments_per_row() {
    let base_bg = Hsla::white();

    // Case A: selection spans three display rows: row 1 [5, end), full row 2, row 3 [0, 7)
    {
        let selection_color = Hsla {
            h: 200.0,
            s: 0.5,
            l: 0.5,
            a: 0.5,
        };
        let player_color = PlayerColor {
            cursor: selection_color,
            background: selection_color,
            selection: selection_color,
        };

        let spanning_selection = SelectionLayout {
            head: DisplayPoint::new(DisplayRow(3), 7),
            cursor_shape: CursorShape::Bar,
            is_newest: true,
            is_local: true,
            range: DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(3), 7),
            active_rows: DisplayRow(1)..DisplayRow(4),
            user_name: None,
        };

        let selections = vec![(player_color, vec![spanning_selection])];
        let result = EditorElement::bg_segments_per_row(
            DisplayRow(0)..DisplayRow(5),
            &selections,
            [].into_iter(),
            base_bg,
        );

        assert_eq!(result.len(), 5);
        assert!(result[0].is_empty());
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[2].len(), 1);
        assert_eq!(result[3].len(), 1);
        assert!(result[4].is_empty());

        assert_eq!(result[1][0].0.start, DisplayPoint::new(DisplayRow(1), 5));
        assert_eq!(result[1][0].0.end.row(), DisplayRow(1));
        assert_eq!(result[1][0].0.end.column(), u32::MAX);
        assert_eq!(result[2][0].0.start, DisplayPoint::new(DisplayRow(2), 0));
        assert_eq!(result[2][0].0.end.row(), DisplayRow(2));
        assert_eq!(result[2][0].0.end.column(), u32::MAX);
        assert_eq!(result[3][0].0.start, DisplayPoint::new(DisplayRow(3), 0));
        assert_eq!(result[3][0].0.end, DisplayPoint::new(DisplayRow(3), 7));
    }

    // Case B: selection ends exactly at the start of row 3, excluding row 3
    {
        let selection_color = Hsla {
            h: 120.0,
            s: 0.5,
            l: 0.5,
            a: 0.5,
        };
        let player_color = PlayerColor {
            cursor: selection_color,
            background: selection_color,
            selection: selection_color,
        };

        let selection = SelectionLayout {
            head: DisplayPoint::new(DisplayRow(2), 0),
            cursor_shape: CursorShape::Bar,
            is_newest: true,
            is_local: true,
            range: DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(3), 0),
            active_rows: DisplayRow(1)..DisplayRow(3),
            user_name: None,
        };

        let selections = vec![(player_color, vec![selection])];
        let result = EditorElement::bg_segments_per_row(
            DisplayRow(0)..DisplayRow(4),
            &selections,
            [].into_iter(),
            base_bg,
        );

        assert_eq!(result.len(), 4);
        assert!(result[0].is_empty());
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[2].len(), 1);
        assert!(result[3].is_empty());

        assert_eq!(result[1][0].0.start, DisplayPoint::new(DisplayRow(1), 5));
        assert_eq!(result[1][0].0.end.row(), DisplayRow(1));
        assert_eq!(result[1][0].0.end.column(), u32::MAX);
        assert_eq!(result[2][0].0.start, DisplayPoint::new(DisplayRow(2), 0));
        assert_eq!(result[2][0].0.end.row(), DisplayRow(2));
        assert_eq!(result[2][0].0.end.column(), u32::MAX);
    }
}
