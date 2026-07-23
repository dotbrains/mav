use super::*;

impl Editor {
    pub(crate) fn manipulate_lines<M>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut manipulate: M,
    ) where
        M: FnMut(&str) -> LineManipulationResult,
    {
        if self.read_only(cx) {
            return;
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = self.buffer.read(cx).snapshot(cx);

        let mut edits = Vec::new();

        let selections = self.selections.all::<Point>(&display_map);
        let mut selections = selections.iter().peekable();
        let mut contiguous_row_selections = Vec::new();
        let mut new_selections = Vec::new();
        let mut added_lines = 0;
        let mut removed_lines = 0;

        while let Some(selection) = selections.next() {
            let (start_row, end_row) = consume_contiguous_rows(
                &mut contiguous_row_selections,
                selection,
                &display_map,
                &mut selections,
            );

            let start_point = Point::new(start_row.0, 0);
            let end_point = Point::new(
                end_row.previous_row().0,
                buffer.line_len(end_row.previous_row()),
            );
            let text = buffer
                .text_for_range(start_point..end_point)
                .collect::<String>();

            let LineManipulationResult {
                new_text,
                line_count_before,
                line_count_after,
            } = manipulate(&text);

            edits.push((start_point..end_point, new_text));

            let start_row =
                MultiBufferRow(start_point.row + added_lines as u32 - removed_lines as u32);
            let end_row = MultiBufferRow(start_row.0 + line_count_after.saturating_sub(1) as u32);
            new_selections.push(Selection {
                id: selection.id,
                start: start_row,
                end: end_row,
                goal: SelectionGoal::None,
                reversed: selection.reversed,
            });

            if line_count_after > line_count_before {
                added_lines += line_count_after - line_count_before;
            } else if line_count_before > line_count_after {
                removed_lines += line_count_before - line_count_after;
            }
        }

        self.transact(window, cx, |this, window, cx| {
            let buffer = this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
                buffer.snapshot(cx)
            });

            let new_selections = new_selections
                .iter()
                .map(|s| {
                    let start_point = Point::new(s.start.0, 0);
                    let end_point = Point::new(s.end.0, buffer.line_len(s.end));
                    Selection {
                        id: s.id,
                        start: buffer.point_to_offset(start_point),
                        end: buffer.point_to_offset(end_point),
                        goal: s.goal,
                        reversed: s.reversed,
                    }
                })
                .collect();

            this.change_selections(Default::default(), window, cx, |s| {
                s.select(new_selections);
            });

            this.request_autoscroll(Autoscroll::fit(), cx);
        });
    }

    pub(crate) fn manipulate_immutable_lines<Fn>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut callback: Fn,
    ) where
        Fn: FnMut(&mut Vec<&str>),
    {
        self.manipulate_lines(window, cx, |text| {
            let mut lines: Vec<&str> = text.split('\n').collect();
            let line_count_before = lines.len();

            callback(&mut lines);

            LineManipulationResult {
                new_text: lines.join("\n"),
                line_count_before,
                line_count_after: lines.len(),
            }
        });
    }

    pub(crate) fn manipulate_mutable_lines<Fn>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut callback: Fn,
    ) where
        Fn: FnMut(&mut Vec<Cow<'_, str>>),
    {
        self.manipulate_lines(window, cx, |text| {
            let mut lines: Vec<Cow<str>> = text.split('\n').map(Cow::from).collect();
            let line_count_before = lines.len();

            callback(&mut lines);

            LineManipulationResult {
                new_text: lines.join("\n"),
                line_count_before,
                line_count_after: lines.len(),
            }
        });
    }

    pub fn convert_indentation_to_spaces(
        &mut self,
        _: &ConvertIndentationToSpaces,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.buffer.read(cx).language_settings(cx);
        let tab_size = settings.tab_size.get() as usize;

        self.manipulate_mutable_lines(window, cx, |lines| {
            let mut reindented_line = String::with_capacity(MAX_LINE_LEN);
            let space_cache: Vec<Vec<char>> = (1..=tab_size)
                .map(|n| IndentSize::spaces(n as u32).chars().collect())
                .collect();

            for line in lines.iter_mut().filter(|line| !line.is_empty()) {
                let mut chars = line.as_ref().chars();
                let mut col = 0;
                let mut changed = false;

                for ch in chars.by_ref() {
                    match ch {
                        ' ' => {
                            reindented_line.push(' ');
                            col += 1;
                        }
                        '\t' => {
                            let spaces_len = tab_size - (col % tab_size);
                            reindented_line.extend(&space_cache[spaces_len - 1]);
                            col += spaces_len;
                            changed = true;
                        }
                        _ => {
                            reindented_line.push(ch);
                            break;
                        }
                    }
                }

                if !changed {
                    reindented_line.clear();
                    continue;
                }
                reindented_line.extend(chars);
                *line = Cow::Owned(reindented_line.clone());
                reindented_line.clear();
            }
        });
    }

    pub fn convert_indentation_to_tabs(
        &mut self,
        _: &ConvertIndentationToTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = self.buffer.read(cx).language_settings(cx);
        let tab_size = settings.tab_size.get() as usize;

        self.manipulate_mutable_lines(window, cx, |lines| {
            let mut reindented_line = String::with_capacity(MAX_LINE_LEN);
            let space_cache: Vec<Vec<char>> = (1..=tab_size)
                .map(|n| IndentSize::spaces(n as u32).chars().collect())
                .collect();

            for line in lines.iter_mut().filter(|line| !line.is_empty()) {
                let mut chars = line.chars();
                let mut spaces_count = 0;
                let mut first_non_indent_char = None;
                let mut changed = false;

                for ch in chars.by_ref() {
                    match ch {
                        ' ' => {
                            spaces_count += 1;
                            changed = true;
                            if spaces_count == tab_size {
                                reindented_line.push('\t');
                                spaces_count = 0;
                            }
                        }
                        '\t' => {
                            reindented_line.push('\t');
                            spaces_count = 0;
                        }
                        _ => {
                            first_non_indent_char = Some(ch);
                            break;
                        }
                    }
                }

                if !changed {
                    reindented_line.clear();
                    continue;
                }
                if spaces_count > 0 {
                    reindented_line.extend(&space_cache[spaces_count - 1]);
                }
                if let Some(extra_char) = first_non_indent_char {
                    reindented_line.push(extra_char);
                }
                reindented_line.extend(chars);
                *line = Cow::Owned(reindented_line.clone());
                reindented_line.clear();
            }
        });
    }
}
