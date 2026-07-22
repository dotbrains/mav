use super::*;

#[gpui::test]
fn test_mouse_to_cell_test(mut rng: StdRng) {
    const ITERATIONS: usize = 10;
    const PRECISION: usize = 1000;

    for _ in 0..ITERATIONS {
        let viewport_cells = rng.random_range(15..20);
        let cell_size = rng.random_range(5 * PRECISION..20 * PRECISION) as f32 / PRECISION as f32;

        let size = crate::TerminalBounds {
            cell_width: Pixels::from(cell_size),
            line_height: Pixels::from(cell_size),
            bounds: bounds(
                GpuiPoint::default(),
                size(
                    Pixels::from(cell_size * (viewport_cells as f32)),
                    Pixels::from(cell_size * (viewport_cells as f32)),
                ),
            ),
        };

        let cells = get_cells(size, &mut rng);
        let content = convert_cells_to_content(size, &cells);

        for row in 0..(viewport_cells - 1) {
            let row = row as usize;
            for col in 0..(viewport_cells - 1) {
                let col = col as usize;

                let row_offset = rng.random_range(0..PRECISION) as f32 / PRECISION as f32;
                let col_offset = rng.random_range(0..PRECISION) as f32 / PRECISION as f32;

                let mouse_pos = point(
                    Pixels::from(col as f32 * cell_size + col_offset),
                    Pixels::from(row as f32 * cell_size + row_offset),
                );

                let content_index = content_index_for_mouse(mouse_pos, &content.terminal_bounds);
                let mouse_cell = content.cells[content_index].character();
                let real_cell = cells[row][col];

                assert_eq!(mouse_cell, real_cell);
            }
        }
    }
}

#[gpui::test]
fn test_mouse_to_cell_clamp(mut rng: StdRng) {
    let size = crate::TerminalBounds {
        cell_width: Pixels::from(10.),
        line_height: Pixels::from(10.),
        bounds: bounds(
            GpuiPoint::default(),
            size(Pixels::from(100.), Pixels::from(100.)),
        ),
    };

    let cells = get_cells(size, &mut rng);
    let content = convert_cells_to_content(size, &cells);

    assert_eq!(
        content.cells[content_index_for_mouse(
            point(Pixels::from(-10.), Pixels::from(-10.)),
            &content.terminal_bounds,
        )]
        .character(),
        cells[0][0]
    );
    assert_eq!(
        content.cells[content_index_for_mouse(
            point(Pixels::from(1000.), Pixels::from(1000.)),
            &content.terminal_bounds,
        )]
        .character(),
        cells[9][9]
    );
}

#[gpui::test]
async fn test_set_size_coalesces_pixel_only_changes(cx: &mut TestAppContext) {
    let builder = cx.update(|cx| {
        TerminalBuilder::new_display_only(
            SettingsCursorShape::Block,
            AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        )
    });
    let mut terminal = builder.terminal;

    let base_bounds = TerminalBounds {
        cell_width: Pixels::from(10.),
        line_height: Pixels::from(10.),
        bounds: bounds(
            GpuiPoint::default(),
            size(Pixels::from(100.), Pixels::from(100.)),
        ),
    };

    terminal.set_size(base_bounds);
    terminal.events.clear();
    assert_eq!(terminal.last_content.terminal_bounds, base_bounds);

    // Pixel-only change: height grows by 1px but still the same number of rows/cols.
    let mut pixel_changed = base_bounds;
    pixel_changed.bounds.size.height = Pixels::from(101.);
    terminal.set_size(pixel_changed);
    assert!(terminal.events.is_empty());
    assert_eq!(terminal.last_content.terminal_bounds, pixel_changed);

    // Grid change: height increases enough to add a row.
    let mut grid_changed = base_bounds;
    grid_changed.bounds.size.height = Pixels::from(110.);
    terminal.set_size(grid_changed);
    assert!(matches!(
        terminal.events.back(),
        Some(InternalEvent::Resize(_))
    ));
}

fn get_cells(size: TerminalBounds, rng: &mut StdRng) -> Vec<Vec<char>> {
    let mut cells = Vec::new();

    for _ in 0..size.num_lines() {
        let mut row_vec = Vec::new();
        for _ in 0..size.num_columns() {
            let cell_char = rng.sample(distr::Alphanumeric) as char;
            row_vec.push(cell_char)
        }
        cells.push(row_vec)
    }

    cells
}

fn convert_cells_to_content(terminal_bounds: TerminalBounds, cells: &[Vec<char>]) -> Content {
    let mut ic = Vec::new();

    for (index, row) in cells.iter().enumerate() {
        for (cell_index, cell_char) in row.iter().enumerate() {
            let mut cell = Cell::default();
            cell.set_character(*cell_char);
            ic.push(IndexedCell {
                point: Point::new(index as i32, cell_index),
                cell,
            });
        }
    }

    Content {
        cells: ic,
        terminal_bounds,
        ..Default::default()
    }
}
