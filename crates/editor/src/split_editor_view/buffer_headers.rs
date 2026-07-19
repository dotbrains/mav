use super::*;

pub(super) struct SplitBufferHeadersElement {
    lhs_editor: Entity<Editor>,
    rhs_editor: Entity<Editor>,
    style: EditorStyle,
}

impl SplitBufferHeadersElement {
    pub(super) fn new(
        lhs_editor: Entity<Editor>,
        rhs_editor: Entity<Editor>,
        style: EditorStyle,
    ) -> Self {
        Self {
            lhs_editor,
            rhs_editor,
            style,
        }
    }
}

struct BufferHeaderLayout {
    element: AnyElement,
}

pub(super) struct SplitBufferHeadersPrepaintState {
    content_bounds: Bounds<Pixels>,
    sticky_header: Option<AnyElement>,
    non_sticky_headers: Vec<BufferHeaderLayout>,
}

impl IntoElement for SplitBufferHeadersElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SplitBufferHeadersElement {
    type RequestLayoutState = ();
    type PrepaintState = SplitBufferHeadersPrepaintState;

    fn id(&self) -> Option<gpui::ElementId> {
        Some("split-buffer-headers".into())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.position = gpui::Position::Absolute;
        style.inset.top = DefiniteLength::Fraction(0.0).into();
        style.inset.left = DefiniteLength::Fraction(0.0).into();
        style.size.width = Length::Definite(DefiniteLength::Fraction(1.0));
        style.size.height = Length::Definite(DefiniteLength::Fraction(1.0));
        let layout_id = window.request_layout(style, [], _cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return SplitBufferHeadersPrepaintState {
                content_bounds: bounds,
                sticky_header: None,
                non_sticky_headers: Vec::new(),
            };
        }

        let rem_size = self.rem_size();
        let text_style = TextStyleRefinement {
            font_size: Some(self.style.text.font_size),
            line_height: Some(self.style.text.line_height),
            ..Default::default()
        };

        window.with_rem_size(rem_size, |window| {
            window.with_text_style(Some(text_style), |window| {
                Self::prepaint_inner(self, bounds, window, cx)
            })
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem_size = self.rem_size();
        let text_style = TextStyleRefinement {
            font_size: Some(self.style.text.font_size),
            line_height: Some(self.style.text.line_height),
            ..Default::default()
        };

        window.with_rem_size(rem_size, |window| {
            window.with_text_style(Some(text_style), |window| {
                window.with_content_mask(
                    Some(ContentMask::new(prepaint.content_bounds)),
                    |window| {
                        for header_layout in &mut prepaint.non_sticky_headers {
                            header_layout.element.paint(window, cx);
                        }

                        if let Some(mut sticky_header) = prepaint.sticky_header.take() {
                            sticky_header.paint(window, cx);
                        }
                    },
                );
            });
        });
    }
}

impl SplitBufferHeadersElement {
    fn rem_size(&self) -> Option<Pixels> {
        match self.style.text.font_size {
            AbsoluteLength::Pixels(pixels) => {
                let rem_size_scale = {
                    let default_font_size_scale = 14. / ui::BASE_REM_SIZE_IN_PX;
                    let default_font_size_delta = 1. - default_font_size_scale;
                    1. + default_font_size_delta
                };

                Some(pixels * rem_size_scale)
            }
            AbsoluteLength::Rems(rems) => Some(rems.to_pixels(ui::BASE_REM_SIZE_IN_PX.into())),
        }
    }

    fn prepaint_inner(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> SplitBufferHeadersPrepaintState {
        let line_height = window.line_height();

        let snapshot = self
            .rhs_editor
            .update(cx, |editor, cx| editor.snapshot(window, cx));
        let scroll_position = snapshot.scroll_position();

        // Compute right margin to avoid overlapping the scrollbar
        let content_bounds = self.content_bounds(bounds, cx);
        let available_width = (content_bounds.size.width
            - self.rhs_editor.read(cx).last_right_margin())
        .max(Pixels::ZERO);

        let visible_height_in_lines = content_bounds.size.height / line_height;
        let max_row = snapshot.max_point().row();
        let start_row = cmp::min(DisplayRow(scroll_position.y.floor() as u32), max_row);
        let end_row = cmp::min(
            (scroll_position.y + visible_height_in_lines as f64).ceil() as u32,
            max_row.next_row().0,
        );
        let end_row = DisplayRow(end_row);

        let (selected_buffer_ids, latest_selection_anchors) =
            self.compute_selection_info(&snapshot, cx);

        let sticky_header = if snapshot.buffer_snapshot().show_headers() {
            snapshot
                .sticky_header_excerpt(scroll_position.y)
                .map(|sticky_excerpt| {
                    self.build_sticky_header(
                        sticky_excerpt,
                        &snapshot,
                        scroll_position,
                        content_bounds,
                        available_width,
                        line_height,
                        &selected_buffer_ids,
                        &latest_selection_anchors,
                        start_row,
                        end_row,
                        window,
                        cx,
                    )
                })
        } else {
            None
        };

        let sticky_header_excerpt_id = snapshot
            .sticky_header_excerpt(scroll_position.y)
            .map(|e| e.excerpt);

        let non_sticky_headers = self.build_non_sticky_headers(
            &snapshot,
            scroll_position,
            content_bounds,
            available_width,
            line_height,
            start_row,
            end_row,
            &selected_buffer_ids,
            &latest_selection_anchors,
            sticky_header_excerpt_id,
            window,
            cx,
        );

        SplitBufferHeadersPrepaintState {
            content_bounds,
            sticky_header,
            non_sticky_headers,
        }
    }

    fn content_bounds(&self, bounds: Bounds<Pixels>, cx: &App) -> Bounds<Pixels> {
        // Left hand side and right hand side horizontal scrollbars are
        // independent, so we clip the bottom if either is visible.
        let horizontal_scrollbar_height =
            (self.lhs_editor.read(cx).last_horizontal_scrollbar_visible()
                || self.rhs_editor.read(cx).last_horizontal_scrollbar_visible())
            .then_some(self.style.scrollbar_width)
            .unwrap_or(Pixels::ZERO);

        Bounds::new(
            bounds.origin,
            size(
                bounds.size.width,
                (bounds.size.height - horizontal_scrollbar_height).max(Pixels::ZERO),
            ),
        )
    }

    fn compute_selection_info(
        &self,
        snapshot: &EditorSnapshot,
        cx: &App,
    ) -> (HashSet<BufferId>, HashMap<BufferId, Anchor>) {
        let editor = self.rhs_editor.read(cx);
        let all_selections = editor
            .selections
            .all::<crate::Point>(&snapshot.display_snapshot);
        let all_anchor_selections = editor.selections.all_anchors(&snapshot.display_snapshot);

        let mut selected_buffer_ids = HashSet::default();
        for selection in &all_selections {
            for buffer_id in snapshot
                .buffer_snapshot()
                .buffer_ids_for_range(selection.range())
            {
                selected_buffer_ids.insert(buffer_id);
            }
        }

        let mut anchors_by_buffer: HashMap<BufferId, (usize, Anchor)> = HashMap::default();
        for selection in all_anchor_selections.iter() {
            let head = selection.head();
            if let Some((text_anchor, _)) = snapshot.buffer_snapshot().anchor_to_buffer_anchor(head)
            {
                anchors_by_buffer
                    .entry(text_anchor.buffer_id)
                    .and_modify(|(latest_id, latest_anchor)| {
                        if selection.id > *latest_id {
                            *latest_id = selection.id;
                            *latest_anchor = head;
                        }
                    })
                    .or_insert((selection.id, head));
            }
        }
        let latest_selection_anchors = anchors_by_buffer
            .into_iter()
            .map(|(buffer_id, (_, anchor))| (buffer_id, anchor))
            .collect();

        (selected_buffer_ids, latest_selection_anchors)
    }

    fn build_sticky_header(
        &self,
        StickyHeaderExcerpt { excerpt }: StickyHeaderExcerpt<'_>,
        snapshot: &EditorSnapshot,
        scroll_position: gpui::Point<ScrollOffset>,
        bounds: Bounds<Pixels>,
        available_width: Pixels,
        line_height: Pixels,
        selected_buffer_ids: &HashSet<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        start_row: DisplayRow,
        end_row: DisplayRow,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let jump_data = header_jump_data(
            snapshot,
            DisplayRow(scroll_position.y as u32),
            FILE_HEADER_HEIGHT + MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
            excerpt,
            latest_selection_anchors,
        );

        let editor_bg_color = cx.theme().colors().editor_background;
        let selected = selected_buffer_ids.contains(&excerpt.buffer_id());

        let mut header = v_flex()
            .id("sticky-buffer-header")
            .w(available_width)
            .relative()
            .child(
                div()
                    .w(available_width)
                    .h(FILE_HEADER_HEIGHT as f32 * line_height)
                    .bg(linear_gradient(
                        0.,
                        linear_color_stop(editor_bg_color.opacity(0.), 0.),
                        linear_color_stop(editor_bg_color, 0.6),
                    ))
                    .absolute()
                    .top_0(),
            )
            .child(
                render_buffer_header(
                    &self.rhs_editor,
                    excerpt,
                    false,
                    selected,
                    true,
                    jump_data,
                    window,
                    cx,
                )
                .into_any_element(),
            )
            .into_any_element();

        let mut origin = bounds.origin;

        for (block_row, block) in snapshot.blocks_in_range(start_row..end_row) {
            if !block.is_buffer_header() {
                continue;
            }

            if block_row.0 <= scroll_position.y as u32 {
                continue;
            }

            let max_row = block_row.0.saturating_sub(FILE_HEADER_HEIGHT);
            let offset = scroll_position.y - max_row as f64;

            if offset > 0.0 {
                origin.y -= Pixels::from(offset * f64::from(line_height));
            }
            break;
        }

        let available_size = size(
            AvailableSpace::Definite(available_width),
            AvailableSpace::MinContent,
        );

        Self::prepaint_header(&mut header, origin, available_size, bounds, window, cx);

        header
    }

    fn build_non_sticky_headers(
        &self,
        snapshot: &EditorSnapshot,
        scroll_position: gpui::Point<ScrollOffset>,
        bounds: Bounds<Pixels>,
        available_width: Pixels,
        line_height: Pixels,
        start_row: DisplayRow,
        end_row: DisplayRow,
        selected_buffer_ids: &HashSet<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        sticky_header: Option<&ExcerptBoundaryInfo>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<BufferHeaderLayout> {
        let mut headers = Vec::new();

        for (block_row, block) in snapshot.blocks_in_range(start_row..end_row) {
            let (excerpt, is_folded) = match block {
                Block::BufferHeader { excerpt, .. } => {
                    if sticky_header == Some(excerpt) {
                        continue;
                    }
                    (excerpt, false)
                }
                Block::FoldedBuffer { first_excerpt, .. } => (first_excerpt, true),
                // ExcerptBoundary is just a separator line, not a buffer header
                Block::ExcerptBoundary { .. } | Block::Custom(_) | Block::Spacer { .. } => continue,
            };

            let selected = selected_buffer_ids.contains(&excerpt.buffer_id());
            let jump_data = header_jump_data(
                snapshot,
                block_row,
                block.height(),
                excerpt,
                latest_selection_anchors,
            );

            let mut header = render_buffer_header(
                &self.rhs_editor,
                excerpt,
                is_folded,
                selected,
                false,
                jump_data,
                window,
                cx,
            )
            .into_any_element();

            let y_offset = (block_row.0 as f64 - scroll_position.y) * f64::from(line_height);
            let origin = point(bounds.origin.x, bounds.origin.y + Pixels::from(y_offset));

            let available_size = size(
                AvailableSpace::Definite(available_width),
                AvailableSpace::MinContent,
            );

            Self::prepaint_header(&mut header, origin, available_size, bounds, window, cx);

            headers.push(BufferHeaderLayout { element: header });
        }

        headers
    }

    fn prepaint_header(
        header: &mut AnyElement,
        origin: gpui::Point<Pixels>,
        available_size: gpui::Size<AvailableSpace>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
            header.prepaint_as_root(origin, available_size, window, cx);
        });
    }
}
