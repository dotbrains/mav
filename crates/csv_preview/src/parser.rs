use crate::{
    CsvPreviewView,
    types::TableLikeContent,
    types::{LineNumber, TableCell},
};
use editor::Editor;
use gpui::{AppContext, Context, Entity, Subscription, Task};
use std::time::{Duration, Instant};
use text::BufferSnapshot;
use ui::{SharedString, table_row::TableRow};

pub(crate) const REPARSE_DEBOUNCE: Duration = Duration::from_millis(200);

pub(crate) struct EditorState {
    pub editor: Entity<Editor>,
    pub _subscription: Subscription,
}

impl CsvPreviewView {
    pub(crate) fn parse_csv_from_active_editor(
        &mut self,
        wait_for_debounce: bool,
        cx: &mut Context<Self>,
    ) {
        let editor = self.active_editor_state.editor.clone();
        self.parsing_task = Some(self.parse_csv_in_background(wait_for_debounce, editor, cx));
    }

    fn parse_csv_in_background(
        &mut self,
        wait_for_debounce: bool,
        editor: Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        cx.spawn(async move |view, cx| {
            if wait_for_debounce {
                // Smart debouncing: check if cooldown period has already passed
                let now = Instant::now();
                let should_wait = view.update(cx, |view, _| {
                    if let Some(last_end) = view.last_parse_end_time {
                        let cooldown_until = last_end + REPARSE_DEBOUNCE;
                        if now < cooldown_until {
                            Some(cooldown_until - now)
                        } else {
                            None // Cooldown already passed, parse immediately
                        }
                    } else {
                        None // First parse, no debounce
                    }
                })?;

                if let Some(wait_duration) = should_wait {
                    cx.background_executor().timer(wait_duration).await;
                }
            }

            let buffer_snapshot = view.update(cx, |_, cx| {
                editor
                    .read(cx)
                    .buffer()
                    .read(cx)
                    .as_singleton()
                    .map(|b| b.read(cx).text_snapshot())
            })?;

            let Some(buffer_snapshot) = buffer_snapshot else {
                return Ok(());
            };

            let instant = Instant::now();
            let parsed_csv = cx
                .background_spawn(async move { from_buffer(&buffer_snapshot) })
                .await;
            let parse_duration = instant.elapsed();
            let parse_end_time: Instant = Instant::now();
            log::debug!("Parsed CSV in {}ms", parse_duration.as_millis());
            view.update(cx, move |view, cx| {
                view.performance_metrics
                    .timings
                    .insert("Parsing", (parse_duration, Instant::now()));

                log::debug!("Parsed {} rows", parsed_csv.rows.len());
                view.engine.contents = parsed_csv;
                view.sync_column_widths(cx);
                view.last_parse_end_time = Some(parse_end_time);

                view.apply_filter_sort();
                cx.notify();
            })
        })
    }
}

pub fn from_buffer(buffer_snapshot: &BufferSnapshot) -> TableLikeContent {
    let text = buffer_snapshot.text();

    if text.trim().is_empty() {
        return TableLikeContent::default();
    }

    let (parsed_cells_with_positions, line_numbers) = parse_csv_with_positions(&text);
    if parsed_cells_with_positions.is_empty() {
        return TableLikeContent::default();
    }
    let raw_headers = parsed_cells_with_positions[0].clone();

    // Calculating the longest row, as CSV might have less headers than max row width
    let Some(max_number_of_cols) = parsed_cells_with_positions.iter().map(|r| r.len()).max() else {
        return TableLikeContent::default();
    };

    // Convert to TableCell objects with buffer positions
    let headers = create_table_row(&buffer_snapshot, max_number_of_cols, raw_headers);

    let rows = parsed_cells_with_positions
        .into_iter()
        .skip(1)
        .map(|row| create_table_row(&buffer_snapshot, max_number_of_cols, row))
        .collect();

    let row_line_numbers = line_numbers.into_iter().skip(1).collect();

    TableLikeContent {
        headers,
        rows,
        line_numbers: row_line_numbers,
        number_of_cols: max_number_of_cols,
    }
}

/// Parse CSV and track byte positions for each cell
fn parse_csv_with_positions(
    text: &str,
) -> (
    Vec<Vec<(SharedString, std::ops::Range<usize>)>>,
    Vec<LineNumber>,
) {
    let mut rows = Vec::new();
    let mut line_numbers = Vec::new();
    let mut current_row: Vec<(SharedString, std::ops::Range<usize>)> = Vec::new();
    let mut current_field = String::new();
    let mut field_start_offset = 0;
    let mut current_offset = 0;
    let mut in_quotes = false;
    let mut current_line = 1; // 1-based line numbering
    let mut row_start_line = 1;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let char_byte_len = ch.len_utf8();

        match ch {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        // Escaped quote
                        chars.next();
                        current_field.push('"');
                        current_offset += 1; // Skip the second quote
                    } else {
                        // End of quoted field
                        in_quotes = false;
                    }
                } else {
                    // Start of quoted field
                    in_quotes = true;
                    if current_field.is_empty() {
                        // Include the opening quote in the range
                        field_start_offset = current_offset;
                    }
                }
            }
            ',' if !in_quotes => {
                // Field separator
                let field_end_offset = current_offset;
                if current_field.is_empty() && !in_quotes {
                    field_start_offset = current_offset;
                }
                current_row.push((
                    current_field.clone().into(),
                    field_start_offset..field_end_offset,
                ));
                current_field.clear();
                field_start_offset = current_offset + char_byte_len;
            }
            '\n' => {
                current_line += 1;
                if !in_quotes {
                    // Row separator (only when not inside quotes)
                    let field_end_offset = current_offset;
                    if current_field.is_empty() && current_row.is_empty() {
                        field_start_offset = 0;
                    }
                    current_row.push((
                        current_field.clone().into(),
                        field_start_offset..field_end_offset,
                    ));
                    current_field.clear();

                    // Only add non-empty rows
                    if !current_row.is_empty()
                        && !current_row.iter().all(|(field, _)| field.trim().is_empty())
                    {
                        rows.push(current_row);
                        // Add line number info for this row
                        let line_info = if row_start_line == current_line - 1 {
                            LineNumber::Line(row_start_line)
                        } else {
                            LineNumber::LineRange(row_start_line, current_line - 1)
                        };
                        line_numbers.push(line_info);
                    }
                    current_row = Vec::new();
                    row_start_line = current_line;
                    field_start_offset = current_offset + char_byte_len;
                } else {
                    // Newline inside quotes - preserve it
                    current_field.push(ch);
                }
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    // Handle Windows line endings (\r\n): account for \r byte, let \n be handled next
                    current_offset += char_byte_len;
                    continue;
                } else {
                    // Standalone \r
                    current_line += 1;
                    if !in_quotes {
                        // Row separator (only when not inside quotes)
                        let field_end_offset = current_offset;
                        current_row.push((
                            current_field.clone().into(),
                            field_start_offset..field_end_offset,
                        ));
                        current_field.clear();

                        // Only add non-empty rows
                        if !current_row.is_empty()
                            && !current_row.iter().all(|(field, _)| field.trim().is_empty())
                        {
                            rows.push(current_row);
                            // Add line number info for this row
                            let line_info = if row_start_line == current_line - 1 {
                                LineNumber::Line(row_start_line)
                            } else {
                                LineNumber::LineRange(row_start_line, current_line - 1)
                            };
                            line_numbers.push(line_info);
                        }
                        current_row = Vec::new();
                        row_start_line = current_line;
                        field_start_offset = current_offset + char_byte_len;
                    } else {
                        // \r inside quotes - preserve it
                        current_field.push(ch);
                    }
                }
            }
            _ => {
                if current_field.is_empty() && !in_quotes {
                    field_start_offset = current_offset;
                }
                current_field.push(ch);
            }
        }

        current_offset += char_byte_len;
    }

    // Add the last field and row if not empty
    if !current_field.is_empty() || !current_row.is_empty() {
        let field_end_offset = current_offset;
        current_row.push((
            current_field.clone().into(),
            field_start_offset..field_end_offset,
        ));
    }
    if !current_row.is_empty() && !current_row.iter().all(|(field, _)| field.trim().is_empty()) {
        rows.push(current_row);
        // Add line number info for the last row
        let line_info = if row_start_line == current_line {
            LineNumber::Line(row_start_line)
        } else {
            LineNumber::LineRange(row_start_line, current_line)
        };
        line_numbers.push(line_info);
    }

    (rows, line_numbers)
}

fn create_table_row(
    buffer_snapshot: &BufferSnapshot,
    max_number_of_cols: usize,
    row: Vec<(SharedString, std::ops::Range<usize>)>,
) -> TableRow<TableCell> {
    let mut raw_row = row
        .into_iter()
        .map(|(content, range)| {
            TableCell::from_buffer_position(content, range.start, range.end, &buffer_snapshot)
        })
        .collect::<Vec<_>>();

    let append_elements = max_number_of_cols - raw_row.len();
    if append_elements > 0 {
        for _ in 0..append_elements {
            raw_row.push(TableCell::Virtual);
        }
    }

    TableRow::from_vec(raw_row, max_number_of_cols)
}

#[cfg(test)]
mod tests;

impl TableLikeContent {
    #[cfg(test)]
    pub fn from_str(text: String) -> Self {
        use text::{Buffer, BufferId, ReplicaId};

        let buffer_id = BufferId::new(1).unwrap();
        let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, text);
        let snapshot = buffer.snapshot();
        from_buffer(snapshot)
    }
}
