use gpui::{App, TextAlign, TextStyleRefinement};
use pulldown_cmark::Alignment;
use ui::prelude::*;

use crate::html::html_parser::{ParsedHtmlTable, ParsedHtmlTableRow};
use crate::html::html_rendering::HtmlSourceAllocator;
use crate::{MarkdownElement, MarkdownElementBuilder};

impl MarkdownElement {
    pub(super) fn render_html_table(
        &self,
        table: &ParsedHtmlTable,
        source_allocator: &mut HtmlSourceAllocator,
        builder: &mut MarkdownElementBuilder,
        markdown_end: usize,
        cx: &mut App,
    ) {
        if let Some(caption) = &table.caption {
            builder.push_div(
                div().when(!self.style.height_is_multiple_of_line_height, |el| {
                    el.mb_2().line_height(rems(1.3))
                }),
                &table.source_range,
                markdown_end,
            );
            self.render_html_paragraph(caption, source_allocator, builder, cx, markdown_end);
            builder.pop_div();
        }

        let actual_header_column_count = html_table_columns_count(&table.header);
        let actual_body_column_count = html_table_columns_count(&table.body);
        let max_column_count = actual_header_column_count.max(actual_body_column_count);

        if max_column_count == 0 {
            return;
        }

        let total_rows = table.header.len() + table.body.len();
        let mut grid_occupied = vec![vec![false; max_column_count]; total_rows];

        builder.push_div(
            div()
                .id(("html-table", table.source_range.start))
                .grid()
                .grid_cols(max_column_count as u16)
                .when(self.style.table_columns_min_size, |this| {
                    this.grid_cols_min_content(max_column_count as u16)
                })
                .when(!self.style.table_columns_min_size, |this| {
                    this.grid_cols(max_column_count as u16)
                })
                .w_full()
                .mb_2()
                .border(px(1.5))
                .border_color(cx.theme().colors().border)
                .rounded_sm()
                .overflow_hidden(),
            &table.source_range,
            markdown_end,
        );

        for (row_index, row) in table.header.iter().chain(table.body.iter()).enumerate() {
            let mut column_index = 0;

            for cell in &row.columns {
                while column_index < max_column_count && grid_occupied[row_index][column_index] {
                    column_index += 1;
                }

                if column_index >= max_column_count {
                    break;
                }

                let max_span = max_column_count.saturating_sub(column_index);
                let text_align = match cell.alignment {
                    Alignment::Left => TextAlign::Left,
                    Alignment::Center => TextAlign::Center,
                    Alignment::Right => TextAlign::Right,
                    _ => self.style.base_text_style.text_align,
                };

                let mut cell_div = div()
                    .col_span(cell.col_span.min(max_span) as u16)
                    .row_span(cell.row_span.min(total_rows - row_index) as u16)
                    .flex()
                    .flex_col()
                    .when(column_index > 0, |this| this.border_l_1())
                    .when(row_index > 0, |this| this.border_t_1())
                    .border_color(cx.theme().colors().border)
                    .px_2()
                    .py_1()
                    .h_full()
                    .when(cell.is_header, |this| {
                        this.bg(cx.theme().colors().title_bar_background)
                    })
                    .when(!cell.is_header && row_index % 2 == 1, |this| {
                        this.bg(cx.theme().colors().panel_background)
                    });

                cell_div = match cell.alignment {
                    Alignment::Center => cell_div.items_center(),
                    Alignment::Right => cell_div.items_end(),
                    _ => cell_div,
                };

                builder.push_text_style(TextStyleRefinement {
                    text_align: Some(text_align),
                    ..Default::default()
                });
                builder.push_div(cell_div, &table.source_range, markdown_end);
                builder.push_div(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .w_full()
                        .justify_center()
                        .text_align(text_align),
                    &table.source_range,
                    markdown_end,
                );
                self.render_html_paragraph(
                    &cell.children,
                    source_allocator,
                    builder,
                    cx,
                    markdown_end,
                );
                builder.pop_div();
                builder.pop_div();
                builder.pop_text_style();

                for row_offset in 0..cell.row_span {
                    for column_offset in 0..cell.col_span {
                        if row_index + row_offset < total_rows
                            && column_index + column_offset < max_column_count
                        {
                            grid_occupied[row_index + row_offset][column_index + column_offset] =
                                true;
                        }
                    }
                }

                column_index += cell.col_span;
            }

            while column_index < max_column_count {
                if grid_occupied[row_index][column_index] {
                    column_index += 1;
                    continue;
                }

                builder.push_div(
                    div()
                        .when(column_index > 0, |this| this.border_l_1())
                        .when(row_index > 0, |this| this.border_t_1())
                        .border_color(cx.theme().colors().border)
                        .when(row_index % 2 == 1, |this| {
                            this.bg(cx.theme().colors().panel_background)
                        }),
                    &table.source_range,
                    markdown_end,
                );
                builder.pop_div();
                column_index += 1;
            }
        }

        builder.pop_div();
    }
}

fn html_table_columns_count(rows: &[ParsedHtmlTableRow]) -> usize {
    let mut actual_column_count = 0;
    for row in rows {
        actual_column_count = actual_column_count.max(
            row.columns
                .iter()
                .map(|column| column.col_span)
                .sum::<usize>(),
        );
    }
    actual_column_count
}
