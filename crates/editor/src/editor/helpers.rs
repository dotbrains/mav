use super::*;

impl Editor {
    pub(crate) fn copy_highlight_json(
        &mut self,
        _: &CopyHighlightJson,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[derive(Serialize)]
        struct Chunk<'a> {
            text: String,
            highlight: Option<&'a str>,
        }

        let snapshot = self.buffer.read(cx).snapshot(cx);
        let mut selection = self.selections.newest::<Point>(&self.display_snapshot(cx));
        let max_point = snapshot.max_point();

        let range = if self.selections.line_mode() {
            selection.start = Point::new(selection.start.row, 0);
            selection.end = cmp::min(max_point, Point::new(selection.end.row + 1, 0));
            selection.goal = SelectionGoal::None;
            selection.range()
        } else if selection.is_empty() {
            Point::new(0, 0)..max_point
        } else {
            selection.range()
        };

        let chunks = snapshot.chunks(
            range,
            LanguageAwareStyling {
                tree_sitter: true,
                diagnostics: true,
            },
        );
        let mut lines = Vec::new();
        let mut line: VecDeque<Chunk> = VecDeque::new();

        let Some(style) = self.style.as_ref() else {
            return;
        };

        for chunk in chunks {
            let highlight = chunk
                .syntax_highlight_id
                .and_then(|id| style.syntax.get_capture_name(id));

            let mut chunk_lines = chunk.text.split('\n').peekable();
            while let Some(text) = chunk_lines.next() {
                let mut merged_with_last_token = false;
                if let Some(last_token) = line.back_mut()
                    && last_token.highlight == highlight
                {
                    last_token.text.push_str(text);
                    merged_with_last_token = true;
                }

                if !merged_with_last_token {
                    line.push_back(Chunk {
                        text: text.into(),
                        highlight,
                    });
                }

                if chunk_lines.peek().is_some() {
                    if line.len() > 1 && line.front().unwrap().text.is_empty() {
                        line.pop_front();
                    }
                    if line.len() > 1 && line.back().unwrap().text.is_empty() {
                        line.pop_back();
                    }

                    lines.push(mem::take(&mut line));
                }
            }
        }

        if line.iter().any(|chunk| !chunk.text.is_empty()) {
            lines.push(line);
        }

        let Some(lines) = serde_json::to_string_pretty(&lines).log_err() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(lines));
    }

    pub fn open_context_menu(
        &mut self,
        _: &OpenContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_autoscroll(Autoscroll::newest(), cx);
        let position = self
            .selections
            .newest_display(&self.display_snapshot(cx))
            .start;
        mouse_context_menu::deploy_context_menu(self, None, position, window, cx);
    }

    pub fn file_header_size(&self) -> u32 {
        FILE_HEADER_HEIGHT
    }

    pub fn restore(
        &mut self,
        revert_changes: HashMap<BufferId, Vec<(Range<text::Anchor>, Rope)>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer().update(cx, |multi_buffer, cx| {
            for (buffer_id, changes) in revert_changes {
                if let Some(buffer) = multi_buffer.buffer(buffer_id) {
                    buffer.update(cx, |buffer, cx| {
                        buffer.edit(
                            changes
                                .into_iter()
                                .map(|(range, text)| (range, text.to_string())),
                            None,
                            cx,
                        );
                    });
                }
            }
        });
        let selections = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx));
        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select(selections);
        });
    }

    pub fn to_pixel_point(
        &mut self,
        source: Anchor,
        editor_snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::Point<Pixels>> {
        let source_point = source.to_display_point(editor_snapshot);
        self.display_to_pixel_point(source_point, editor_snapshot, window, cx)
    }

    pub fn display_to_pixel_point(
        &mut self,
        source: DisplayPoint,
        editor_snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::Point<Pixels>> {
        let line_height = self.style(cx).text.line_height_in_pixels(window.rem_size());
        let text_layout_details = self.text_layout_details(window, cx);
        let scroll_top = text_layout_details
            .scroll_anchor
            .scroll_position(editor_snapshot)
            .y;

        if source.row().as_f64() < scroll_top.floor() {
            return None;
        }
        let source_x = editor_snapshot.x_for_display_point(source, &text_layout_details);
        let source_y = line_height * (source.row().as_f64() - scroll_top) as f32;
        Some(gpui::Point::new(source_x, source_y))
    }

    pub fn register_addon<T: Addon>(&mut self, instance: T) {
        if self.mode.is_minimap() {
            return;
        }
        self.addons
            .insert(std::any::TypeId::of::<T>(), Box::new(instance));
    }

    pub fn unregister_addon<T: Addon>(&mut self) {
        self.addons.remove(&std::any::TypeId::of::<T>());
    }

    pub fn addon<T: Addon>(&self) -> Option<&T> {
        let type_id = std::any::TypeId::of::<T>();
        self.addons
            .get(&type_id)
            .and_then(|item| item.to_any().downcast_ref::<T>())
    }

    pub fn addon_mut<T: Addon>(&mut self) -> Option<&mut T> {
        let type_id = std::any::TypeId::of::<T>();
        self.addons
            .get_mut(&type_id)
            .and_then(|item| item.to_any_mut()?.downcast_mut::<T>())
    }

    pub(crate) fn character_dimensions(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> CharacterDimensions {
        let text_layout_details = self.text_layout_details(window, cx);
        let style = &text_layout_details.editor_style;
        let font_id = window.text_system().resolve_font(&style.text.font());
        let font_size = style.text.font_size.to_pixels(window.rem_size());
        let line_height = style.text.line_height_in_pixels(window.rem_size());
        let em_width = window.text_system().em_width(font_id, font_size).unwrap();
        let em_advance = window.text_system().em_advance(font_id, font_size).unwrap();

        CharacterDimensions {
            em_width,
            em_advance,
            line_height,
        }
    }

    pub fn wait_for_diff_to_load(&self) -> Option<Shared<Task<()>>> {
        self.load_diff_task.clone()
    }
}
