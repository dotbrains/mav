use super::*;

#[test]
fn test_compute_indent_guides() {
    fn assert_compute_indent_guides(
        input: &[usize],
        offset: usize,
        includes_trailing_indent: bool,
        expected: Vec<IndentGuideLayout>,
    ) {
        use std::collections::HashSet;
        assert_eq!(
            compute_indent_guides(input, offset, includes_trailing_indent)
                .into_vec()
                .into_iter()
                .collect::<HashSet<_>>(),
            expected.into_iter().collect::<HashSet<_>>(),
        );
    }

    assert_compute_indent_guides(
        &[0, 1, 2, 2, 1, 0],
        0,
        false,
        vec![
            IndentGuideLayout {
                offset: Point::new(0, 1),
                length: 4,
                continues_offscreen: false,
            },
            IndentGuideLayout {
                offset: Point::new(1, 2),
                length: 2,
                continues_offscreen: false,
            },
        ],
    );

    assert_compute_indent_guides(
        &[2, 2, 2, 1, 1],
        0,
        false,
        vec![
            IndentGuideLayout {
                offset: Point::new(0, 0),
                length: 5,
                continues_offscreen: false,
            },
            IndentGuideLayout {
                offset: Point::new(1, 0),
                length: 3,
                continues_offscreen: false,
            },
        ],
    );

    assert_compute_indent_guides(
        &[1, 2, 3, 2, 1],
        0,
        false,
        vec![
            IndentGuideLayout {
                offset: Point::new(0, 0),
                length: 5,
                continues_offscreen: false,
            },
            IndentGuideLayout {
                offset: Point::new(1, 1),
                length: 3,
                continues_offscreen: false,
            },
            IndentGuideLayout {
                offset: Point::new(2, 2),
                length: 1,
                continues_offscreen: false,
            },
        ],
    );

    assert_compute_indent_guides(
        &[0, 1, 0],
        0,
        true,
        vec![IndentGuideLayout {
            offset: Point::new(0, 1),
            length: 1,
            continues_offscreen: false,
        }],
    );

    assert_compute_indent_guides(
        &[0, 1, 1],
        0,
        true,
        vec![IndentGuideLayout {
            offset: Point::new(0, 1),
            length: 1,
            continues_offscreen: true,
        }],
    );
    assert_compute_indent_guides(
        &[0, 1, 2],
        0,
        true,
        vec![IndentGuideLayout {
            offset: Point::new(0, 1),
            length: 1,
            continues_offscreen: true,
        }],
    );
}
