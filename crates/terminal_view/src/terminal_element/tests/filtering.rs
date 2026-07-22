use super::super::*;

#[test]
fn test_screen_position_filtering_with_positive_lines() {
    // Test the unified screen-position-based filtering approach.
    // This works for both Scrollable and Inline modes because we filter
    // by enumerated line group index, not by cell.point.line values.
    use itertools::Itertools;
    use terminal::{Cell, IndexedCell, Point};

    // Create mock cells for lines 0-23 (typical terminal with 24 visible lines)
    let mut cells = Vec::new();
    for line in 0..24i32 {
        for col in 0..3i32 {
            cells.push(IndexedCell {
                point: Point::new(line, col as usize),
                cell: Cell::default(),
            });
        }
    }

    // Scenario: Terminal partially scrolled above viewport
    // First 5 lines (0-4) are clipped, lines 5-15 should be visible
    let rows_above_viewport = 5usize;
    let visible_row_count = 11usize;

    // Apply the same filtering logic as in the render code
    let filtered: Vec<_> = cells
        .iter()
        .chunk_by(|c| c.point.line)
        .into_iter()
        .skip(rows_above_viewport)
        .take(visible_row_count)
        .flat_map(|(_, line_cells)| line_cells)
        .collect();

    // Should have lines 5-15 (11 lines * 3 cells each = 33 cells)
    assert_eq!(filtered.len(), 11 * 3, "Should have 33 cells for 11 lines");

    // First filtered cell should be line 5
    assert_eq!(
        filtered.first().unwrap().point.line,
        5,
        "First cell should be on line 5"
    );

    // Last filtered cell should be line 15
    assert_eq!(
        filtered.last().unwrap().point.line,
        15,
        "Last cell should be on line 15"
    );
}

#[test]
fn test_screen_position_filtering_with_negative_lines() {
    // This is the key test! In Scrollable mode, cells have NEGATIVE line numbers
    // for scrollback history. The screen-position filtering approach works because
    // we filter by enumerated line group index, not by cell.point.line values.
    use itertools::Itertools;
    use terminal::{Cell, IndexedCell, Point};

    // Simulate cells from a scrolled terminal with scrollback
    // These have negative line numbers representing scrollback history
    let mut scrollback_cells = Vec::new();
    for line in -588i32..=-578i32 {
        for col in 0..80i32 {
            scrollback_cells.push(IndexedCell {
                point: Point::new(line, col as usize),
                cell: Cell::default(),
            });
        }
    }

    // Scenario: First 3 screen rows clipped, show next 5 rows
    let rows_above_viewport = 3usize;
    let visible_row_count = 5usize;

    // Apply the same filtering logic as in the render code
    let filtered: Vec<_> = scrollback_cells
        .iter()
        .chunk_by(|c| c.point.line)
        .into_iter()
        .skip(rows_above_viewport)
        .take(visible_row_count)
        .flat_map(|(_, line_cells)| line_cells)
        .collect();

    // Should have 5 lines * 80 cells = 400 cells
    assert_eq!(filtered.len(), 5 * 80, "Should have 400 cells for 5 lines");

    // First filtered cell should be line -585 (skipped 3 lines from -588)
    assert_eq!(
        filtered.first().unwrap().point.line,
        -585,
        "First cell should be on line -585"
    );

    // Last filtered cell should be line -581 (5 lines: -585, -584, -583, -582, -581)
    assert_eq!(
        filtered.last().unwrap().point.line,
        -581,
        "Last cell should be on line -581"
    );
}

#[test]
fn test_screen_position_filtering_skip_all() {
    // Test what happens when we skip more rows than exist
    use itertools::Itertools;
    use terminal::{Cell, IndexedCell, Point};

    let mut cells = Vec::new();
    for line in 0..10i32 {
        cells.push(IndexedCell {
            point: Point::new(line, 0),
            cell: Cell::default(),
        });
    }

    // Skip more rows than exist
    let rows_above_viewport = 100usize;
    let visible_row_count = 5usize;

    let filtered: Vec<_> = cells
        .iter()
        .chunk_by(|c| c.point.line)
        .into_iter()
        .skip(rows_above_viewport)
        .take(visible_row_count)
        .flat_map(|(_, line_cells)| line_cells)
        .collect();

    assert_eq!(
        filtered.len(),
        0,
        "Should have no cells when all are skipped"
    );
}

#[test]
fn test_layout_grid_positioning_math() {
    // Test the math that layout_grid uses for positioning.
    // When we skip N rows, we pass N as start_line_offset to layout_grid,
    // which positions the first visible line at screen row N.

    // Scenario: Terminal at y=-100px, line_height=20px
    // First 5 screen rows are above viewport (clipped)
    // So we skip 5 rows and pass offset=5 to layout_grid

    let terminal_origin_y = -100.0f32;
    let line_height = 20.0f32;
    let rows_skipped = 5;

    // The first visible line (at offset 5) renders at:
    // y = terminal_origin + offset * line_height = -100 + 5*20 = 0
    let first_visible_y = terminal_origin_y + rows_skipped as f32 * line_height;
    assert_eq!(
        first_visible_y, 0.0,
        "First visible line should be at viewport top (y=0)"
    );

    // The 6th visible line (at offset 10) renders at:
    let sixth_visible_y = terminal_origin_y + (rows_skipped + 5) as f32 * line_height;
    assert_eq!(
        sixth_visible_y, 100.0,
        "6th visible line should be at y=100"
    );
}

#[test]
fn test_unified_filtering_works_for_both_modes() {
    // This test proves that the unified screen-position filtering approach
    // works for BOTH positive line numbers (Inline mode) and negative line
    // numbers (Scrollable mode with scrollback).
    //
    // The key insight: we filter by enumerated line group index (screen position),
    // not by cell.point.line values. This makes the filtering agnostic to the
    // actual line numbers in the cells.
    use itertools::Itertools;
    use terminal::Point;
    use terminal::{Cell, IndexedCell};

    // Test with positive line numbers (Inline mode style)
    let positive_cells: Vec<_> = (0..10i32)
        .flat_map(|line| {
            (0..3i32).map(move |col| IndexedCell {
                point: Point::new(line, col as usize),
                cell: Cell::default(),
            })
        })
        .collect();

    // Test with negative line numbers (Scrollable mode with scrollback)
    let negative_cells: Vec<_> = (-10i32..0i32)
        .flat_map(|line| {
            (0..3i32).map(move |col| IndexedCell {
                point: Point::new(line, col as usize),
                cell: Cell::default(),
            })
        })
        .collect();

    let rows_to_skip = 3usize;
    let rows_to_take = 4usize;

    // Filter positive cells
    let positive_filtered: Vec<_> = positive_cells
        .iter()
        .chunk_by(|c| c.point.line)
        .into_iter()
        .skip(rows_to_skip)
        .take(rows_to_take)
        .flat_map(|(_, cells)| cells)
        .collect();

    // Filter negative cells
    let negative_filtered: Vec<_> = negative_cells
        .iter()
        .chunk_by(|c| c.point.line)
        .into_iter()
        .skip(rows_to_skip)
        .take(rows_to_take)
        .flat_map(|(_, cells)| cells)
        .collect();

    // Both should have same count: 4 lines * 3 cells = 12
    assert_eq!(positive_filtered.len(), 12);
    assert_eq!(negative_filtered.len(), 12);

    // Positive: lines 3, 4, 5, 6
    assert_eq!(positive_filtered.first().unwrap().point.line, 3);
    assert_eq!(positive_filtered.last().unwrap().point.line, 6);

    // Negative: lines -7, -6, -5, -4
    assert_eq!(negative_filtered.first().unwrap().point.line, -7);
    assert_eq!(negative_filtered.last().unwrap().point.line, -4);
}
