use super::*;

impl TerminalElement {
    pub fn new(
        terminal: Entity<Terminal>,
        terminal_view: Entity<TerminalView>,
        workspace: WeakEntity<Workspace>,
        focus: FocusHandle,
        focused: bool,
        cursor_visible: bool,
        block_below_cursor: Option<Rc<BlockProperties>>,
        mode: TerminalMode,
    ) -> TerminalElement {
        TerminalElement {
            terminal,
            terminal_view,
            workspace,
            focused,
            focus: focus.clone(),
            cursor_visible,
            block_below_cursor,
            mode,
            interactivity: Default::default(),
        }
        .track_focus(&focus)
    }

    pub fn layout_grid<T: TerminalLayoutCell>(
        grid: impl Iterator<Item = T>,
        start_line_offset: i32,
        text_style: &TextStyle,
        hyperlink: Option<(HighlightStyle, &Range)>,
        minimum_contrast: f32,
        cx: &App,
    ) -> (Vec<LayoutRect>, Vec<BatchedTextRun>) {
        let start_time = Instant::now();
        let theme = cx.theme();

        // Pre-allocate with estimated capacity to reduce reallocations
        let estimated_cells = grid.size_hint().0;
        let estimated_runs = estimated_cells / 10; // Estimate ~10 cells per run
        let estimated_regions = estimated_cells / 20; // Estimate ~20 cells per background region

        let mut batched_runs = Vec::with_capacity(estimated_runs);
        let mut cell_count = 0;

        // Collect background regions for efficient merging
        let mut background_regions: Vec<BackgroundRegion> = Vec::with_capacity(estimated_regions);
        let mut current_batch: Option<BatchedTextRun> = None;

        // First pass: collect all cells and their backgrounds
        let linegroups = grid.into_iter().chunk_by(|cell| cell.point().line);
        for (line_index, (_, line)) in linegroups.into_iter().enumerate() {
            let display_line = start_line_offset + line_index as i32;

            // Flush any existing batch at line boundaries
            if let Some(batch) = current_batch.take() {
                batched_runs.push(batch);
            }

            let mut previous_cell_had_extras = false;

            for cell in line {
                let point = cell.point();
                let cell = cell.cell();
                let mut fg = cell.foreground();
                let mut bg = cell.background();
                if cell.is_inverse() {
                    mem::swap(&mut fg, &mut bg);
                }

                // Collect background regions (skip default background)
                if !is_default_background_color(bg) {
                    let color = convert_color(&bg, theme);
                    let col = point.column as i32;

                    // Try to extend the last region if it's on the same line with the same color
                    if let Some(last_region) = background_regions.last_mut()
                        && last_region.color == color
                        && last_region.start_line == display_line
                        && last_region.end_line == display_line
                        && last_region.end_col + 1 == col
                    {
                        last_region.end_col = col;
                    } else {
                        background_regions.push(BackgroundRegion::new(display_line, col, color));
                    }
                }
                // Skip wide character spacers - they're just placeholders for the second cell of wide characters
                if cell.is_wide_char_spacer() {
                    continue;
                }

                // Skip spaces that follow cells with extras (emoji variation sequences)
                if cell.character() == ' ' && previous_cell_had_extras {
                    previous_cell_had_extras = false;
                    continue;
                }
                // Update tracking for next iteration
                previous_cell_had_extras =
                    matches!(cell.zerowidth(), Some(chars) if !chars.is_empty());

                //Layout current cell text
                {
                    if !is_blank(cell) {
                        cell_count += 1;
                        let cell_style = TerminalElement::cell_style(
                            point,
                            cell,
                            fg,
                            bg,
                            theme,
                            text_style,
                            hyperlink,
                            minimum_contrast,
                        );

                        let cell_point = LayoutPoint::new(display_line, point.column as i32);
                        let zero_width_chars = cell.zerowidth();

                        // Try to batch with existing run
                        if let Some(ref mut batch) = current_batch {
                            if batch.can_append(&cell_style)
                                && batch.start_point.line == cell_point.line
                                && batch.start_point.column + batch.cell_count as i32
                                    == cell_point.column
                            {
                                batch.append_char(cell.character());
                                if let Some(chars) = zero_width_chars {
                                    batch.append_zero_width_chars(chars);
                                }
                            } else {
                                // Flush current batch and start new one
                                let old_batch = current_batch.take().unwrap();
                                batched_runs.push(old_batch);
                                let mut new_batch = BatchedTextRun::new_from_char(
                                    cell_point,
                                    cell.character(),
                                    cell_style,
                                    text_style.font_size,
                                );
                                if let Some(chars) = zero_width_chars {
                                    new_batch.append_zero_width_chars(chars);
                                }
                                current_batch = Some(new_batch);
                            }
                        } else {
                            // Start new batch
                            let mut new_batch = BatchedTextRun::new_from_char(
                                cell_point,
                                cell.character(),
                                cell_style,
                                text_style.font_size,
                            );
                            if let Some(chars) = zero_width_chars {
                                new_batch.append_zero_width_chars(chars);
                            }
                            current_batch = Some(new_batch);
                        }
                    };
                }
            }
        }

        // Flush any remaining batch
        if let Some(batch) = current_batch {
            batched_runs.push(batch);
        }

        // Second pass: merge background regions and convert to layout rects
        let region_count = background_regions.len();
        let merged_regions = merge_background_regions(background_regions);
        let mut rects = Vec::with_capacity(merged_regions.len() * 2); // Estimate 2 rects per merged region

        // Convert merged regions to layout rects
        // Since LayoutRect only supports single-line rectangles, we need to split multi-line regions
        for region in merged_regions {
            for line in region.start_line..=region.end_line {
                rects.push(LayoutRect::new(
                    LayoutPoint::new(line, region.start_col),
                    (region.end_col - region.start_col + 1) as usize,
                    region.color,
                ));
            }
        }

        let layout_time = start_time.elapsed();

        log::debug!(
            "Terminal layout_grid: {} cells processed, \
            {} batched runs created, {} rects (from {} merged regions), \
            layout took {:?}",
            cell_count,
            batched_runs.len(),
            rects.len(),
            region_count,
            layout_time
        );

        (rects, batched_runs)
    }
}
