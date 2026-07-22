use super::*;

#[derive(Default)]
pub(super) struct TableState {
    pub(super) alignments: Vec<Alignment>,
    pub(super) in_head: bool,
    pub(super) row_index: usize,
    pub(super) col_index: usize,
}

impl TableState {
    pub(super) fn start(&mut self, alignments: Vec<Alignment>) {
        self.alignments = alignments;
        self.in_head = false;
        self.row_index = 0;
        self.col_index = 0;
    }

    pub(super) fn end(&mut self) {
        self.alignments.clear();
        self.in_head = false;
        self.row_index = 0;
        self.col_index = 0;
    }

    pub(super) fn start_head(&mut self) {
        self.in_head = true;
    }

    pub(super) fn end_head(&mut self) {
        self.in_head = false;
    }

    pub(super) fn start_row(&mut self) {
        self.col_index = 0;
    }

    pub(super) fn end_row(&mut self) {
        self.row_index += 1;
    }

    pub(super) fn end_cell(&mut self) {
        self.col_index += 1;
    }

    pub(super) fn current_cell_alignment(&self) -> Option<Alignment> {
        if self.alignments.is_empty() {
            return None;
        }
        if self.in_head {
            return Some(Alignment::Center);
        }
        self.alignments.get(self.col_index).copied()
    }
}

pub(super) fn alignment_to_text_align(alignment: Alignment) -> Option<TextAlign> {
    match alignment {
        Alignment::Left => Some(TextAlign::Left),
        Alignment::Center => Some(TextAlign::Center),
        Alignment::Right => Some(TextAlign::Right),
        Alignment::None => None,
    }
}

pub(super) struct MetadataCellStyle {
    pub(super) row_index: usize,
    pub(super) is_key: bool,
}

pub(super) struct MarkdownElementBuilder {
    pub(super) div_stack: Vec<DivStackEntry>,
    pub(super) rendered_lines: Vec<RenderedLine>,
    pub(super) pending_line: PendingLine,
    pub(super) rendered_links: Vec<RenderedLink>,
    pub(super) rendered_footnote_refs: Vec<RenderedFootnoteRef>,
    pub(super) current_source_index: usize,
    pub(super) html_comment: bool,
    pub(super) rendered_footnote_separator: bool,
    pub(super) base_text_style: TextStyle,
    pub(super) text_style_stack: Vec<TextStyleRefinement>,
    pub(super) code_block_stack: Vec<Option<Arc<Language>>>,
    pub(super) link_depth: usize,
    pub(super) list_stack: Vec<ListStackEntry>,
    pub(super) table: TableState,
    pub(super) syntax_theme: Arc<SyntaxTheme>,
}

struct DivStackEntry {
    div: AnyDiv,
    line_break_mode: LineBreakMode,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LineBreakMode {
    TextLayout,
    FlexWrap,
}

impl DivStackEntry {
    pub(super) fn new(div: impl Into<AnyDiv>) -> Self {
        Self {
            div: div.into(),
            line_break_mode: LineBreakMode::TextLayout,
        }
    }
}

#[derive(Default)]
struct PendingLine {
    text: String,
    runs: Vec<TextRun>,
    source_mappings: Vec<SourceMapping>,
}

struct ListStackEntry {
    bullet_index: Option<u64>,
}

impl MarkdownElementBuilder {
    pub(super) fn new(
        container_style: &StyleRefinement,
        base_text_style: TextStyle,
        syntax_theme: Arc<SyntaxTheme>,
    ) -> Self {
        Self {
            div_stack: vec![{
                let mut base_div = div();
                base_div.style().refine(container_style);
                DivStackEntry::new(base_div.debug_selector(|| "inner".into()))
            }],
            rendered_lines: Vec::new(),
            pending_line: PendingLine::default(),
            rendered_links: Vec::new(),
            rendered_footnote_refs: Vec::new(),
            current_source_index: 0,
            html_comment: false,
            rendered_footnote_separator: false,
            base_text_style,
            text_style_stack: Vec::new(),
            code_block_stack: Vec::new(),
            link_depth: 0,
            list_stack: Vec::new(),
            table: TableState::default(),
            syntax_theme,
        }
    }

    pub(super) fn push_text_style(&mut self, style: TextStyleRefinement) {
        self.text_style_stack.push(style);
    }

    pub(super) fn text_style(&self) -> TextStyle {
        let mut style = self.base_text_style.clone();
        for refinement in &self.text_style_stack {
            style.refine(refinement);
        }
        style
    }

    pub(super) fn pop_text_style(&mut self) {
        self.text_style_stack.pop();
    }

    pub(super) fn push_div(
        &mut self,
        div: impl Into<AnyDiv>,
        range: &Range<usize>,
        markdown_end: usize,
    ) {
        let mut div = div.into();
        self.flush_text();

        if range.start == 0 {
            // Remove the top margin on the first element.
            div.style().refine(&StyleRefinement {
                margin: gpui::EdgesRefinement {
                    top: Some(Length::Definite(px(0.).into())),
                    left: None,
                    right: None,
                    bottom: None,
                },
                ..Default::default()
            });
        }

        if range.end == markdown_end {
            div.style().refine(&StyleRefinement {
                margin: gpui::EdgesRefinement {
                    top: None,
                    left: None,
                    right: None,
                    bottom: Some(Length::Definite(rems(0.).into())),
                },
                ..Default::default()
            });
        }

        self.div_stack.push(DivStackEntry::new(div));
    }

    pub(super) fn push_root_block(&mut self, range: &Range<usize>, markdown_end: usize) {
        self.push_div(
            div().group("markdown-root-block").relative(),
            range,
            markdown_end,
        );
        self.push_div(div().pl_4(), range, markdown_end);
    }

    pub(super) fn push_image_child(&mut self, child: impl IntoElement) {
        self.modify_current_div(|el| el.flex().flex_row().flex_wrap().items_start());
        self.div_stack.last_mut().unwrap().line_break_mode = LineBreakMode::FlexWrap;
        self.append_child(child.into_any_element());
    }

    pub(super) fn push_line_break(&mut self, source_range: Range<usize>) {
        if self.uses_flex_line_breaks() {
            self.modify_current_div(|el| el.child(div().w_full().h_0()));
        } else {
            self.push_text("\n", source_range);
        }
    }

    pub(super) fn push_soft_break(&mut self, source_range: Range<usize>) {
        // A soft break right after an item in flex wrap container would otherwise
        // render as a stray leading space before the next wrapped item.
        if self.uses_flex_line_breaks() && self.pending_line.text.is_empty() {
            return;
        }
        self.push_text(" ", source_range);
    }

    pub(super) fn append_child(&mut self, child: AnyElement) {
        self.div_stack.last_mut().unwrap().div.extend([child]);
    }

    pub(super) fn uses_flex_line_breaks(&self) -> bool {
        self.div_stack
            .last()
            .is_some_and(|entry| entry.line_break_mode == LineBreakMode::FlexWrap)
    }

    pub(super) fn modify_current_div(&mut self, f: impl FnOnce(AnyDiv) -> AnyDiv) {
        self.flush_text();
        if let Some(mut entry) = self.div_stack.pop() {
            entry.div = f(entry.div);
            self.div_stack.push(entry);
        }
    }

    pub(super) fn pop_root_block(
        &mut self,
        is_active: bool,
        active_gutter_color: Hsla,
        hovered_gutter_color: Hsla,
    ) {
        self.pop_div();
        self.modify_current_div(|el| {
            el.child(
                div()
                    .h_full()
                    .w(px(4.0))
                    .when(is_active, |this| this.bg(active_gutter_color))
                    .group_hover("markdown-root-block", |this| {
                        if is_active {
                            this
                        } else {
                            this.bg(hovered_gutter_color)
                        }
                    })
                    .rounded_xs()
                    .absolute()
                    .left_0()
                    .top_0(),
            )
        });
        self.pop_div();
    }

    pub(super) fn pop_div(&mut self) {
        self.flush_text();
        let div = self.div_stack.pop().unwrap().div.into_any_element();
        self.append_child(div);
    }

    pub(super) fn push_sourced_element(
        &mut self,
        source_range: Range<usize>,
        element: impl Into<AnyElement>,
    ) {
        self.flush_text();
        let anchor = self.render_source_anchor(source_range);
        self.append_child(
            div()
                .relative()
                .child(anchor)
                .child(element.into())
                .into_any_element(),
        );
    }

    pub(super) fn push_list(&mut self, bullet_index: Option<u64>) {
        self.list_stack.push(ListStackEntry { bullet_index });
    }

    pub(super) fn next_bullet_index(&mut self) -> Option<u64> {
        self.list_stack.last_mut().and_then(|entry| {
            let item_index = entry.bullet_index.as_mut()?;
            *item_index += 1;
            Some(*item_index - 1)
        })
    }

    pub(super) fn pop_list(&mut self) {
        self.list_stack.pop();
    }

    pub(super) fn push_code_block(&mut self, language: Option<Arc<Language>>) {
        self.code_block_stack.push(language);
    }

    pub(super) fn pop_code_block(&mut self) {
        self.code_block_stack.pop();
    }

    pub(super) fn push_link(&mut self, destination_url: SharedString, source_range: Range<usize>) {
        self.rendered_links.push(RenderedLink {
            source_range,
            destination_url,
        });
    }

    pub(super) fn push_footnote_ref(&mut self, label: SharedString, source_range: Range<usize>) {
        self.rendered_footnote_refs.push(RenderedFootnoteRef {
            source_range,
            label,
        });
    }

    pub(super) fn push_text(&mut self, text: &str, source_range: Range<usize>) {
        self.pending_line.source_mappings.push(SourceMapping {
            rendered_index: self.pending_line.text.len(),
            source_index: source_range.start,
        });
        self.pending_line.text.push_str(text);
        self.current_source_index = source_range.end;

        // Compute the base text style once
        let text_style = self.text_style();

        if let Some(Some(language)) = self.code_block_stack.last() {
            let mut offset = 0;
            for (range, highlight_id) in language.highlight_text(&Rope::from(text), 0..text.len()) {
                if range.start > offset {
                    self.pending_line
                        .runs
                        .push(text_style.to_run(range.start - offset));
                }

                let run_len = range.len();
                if let Some(highlight) = self.syntax_theme.get(highlight_id).cloned() {
                    self.pending_line
                        .runs
                        .push(text_style.clone().highlight(highlight).to_run(run_len));
                } else {
                    self.pending_line.runs.push(text_style.to_run(run_len));
                }
                offset = range.end;
            }

            if offset < text.len() {
                self.pending_line
                    .runs
                    .push(text_style.to_run(text.len() - offset));
            }
        } else {
            self.pending_line.runs.push(text_style.to_run(text.len()));
        }
    }

    pub(super) fn trim_trailing_newline(&mut self) {
        if self.pending_line.text.ends_with('\n') {
            self.pending_line
                .text
                .truncate(self.pending_line.text.len() - 1);
            self.pending_line.runs.last_mut().unwrap().len -= 1;
            self.current_source_index -= 1;
        }
    }

    pub(super) fn replace_pending_checkbox(&mut self, on_toggle: Option<CheckboxToggleCallback>) {
        let text = &self.pending_line.text;
        let trimmed = text.trim();
        if trimmed != "[x]" && trimmed != "[X]" && trimmed != "[ ]" {
            return;
        }
        let checked = trimmed != "[ ]";

        let leading_ws = text.len() - text.trim_start().len();
        let marker_rendered = leading_ws..leading_ws + trimmed.len();
        let marker_source = self
            .source_range_for_rendered(&marker_rendered)
            .expect("pending checkbox text must have source mappings");

        self.pending_line = PendingLine::default();

        let toggle_state = if checked {
            ToggleState::Selected
        } else {
            ToggleState::Unselected
        };
        let checkbox = Checkbox::new(
            ElementId::Name(
                format!(
                    "table_checkbox_{}_{}",
                    marker_source.start, marker_source.end
                )
                .into(),
            ),
            toggle_state,
        )
        .fill();

        let checkbox = if let Some(on_toggle) = on_toggle {
            checkbox
                .on_click(move |_state, window, cx| {
                    on_toggle(marker_source.clone(), !checked, window, cx);
                })
                .into_any_element()
        } else {
            checkbox.visualization_only(true).into_any_element()
        };

        let mut checkbox_container = h_flex().w_full();
        checkbox_container = match self.text_style().text_align {
            TextAlign::Left => checkbox_container.justify_start(),
            TextAlign::Center => checkbox_container.justify_center(),
            TextAlign::Right => checkbox_container.justify_end(),
        };

        self.append_child(checkbox_container.child(checkbox).into_any_element());
    }
}
