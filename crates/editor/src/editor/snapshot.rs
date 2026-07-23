use super::*;

#[derive(Clone)]
pub struct EditorSnapshot {
    pub mode: EditorMode,
    pub(crate) show_gutter: bool,
    pub(crate) offset_content: bool,
    pub(crate) show_line_numbers: Option<bool>,
    pub(crate) number_deleted_lines: bool,
    pub(crate) show_git_diff_gutter: Option<bool>,
    pub(crate) show_code_actions: Option<bool>,
    pub(crate) show_runnables: Option<bool>,
    pub(crate) show_breakpoints: Option<bool>,
    pub(crate) show_bookmarks: Option<bool>,
    pub(crate) git_blame_gutter_max_author_length: Option<usize>,
    pub display_snapshot: DisplaySnapshot,
    pub placeholder_display_snapshot: Option<DisplaySnapshot>,
    pub(crate) is_focused: bool,
    pub(crate) scroll_anchor: SharedScrollAnchor,
    pub(crate) ongoing_scroll: OngoingScroll,
    pub(crate) current_line_highlight: CurrentLineHighlight,
    pub(crate) gutter_hovered: bool,
    pub(crate) semantic_tokens_enabled: bool,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct GutterDimensions {
    pub left_padding: Pixels,
    pub right_padding: Pixels,
    pub width: Pixels,
    pub margin: Pixels,
    pub git_blame_entries_width: Option<Pixels>,
}

impl GutterDimensions {
    pub(crate) fn default_with_margin(font_id: FontId, font_size: Pixels, cx: &App) -> Self {
        Self {
            margin: Self::default_gutter_margin(font_id, font_size, cx),
            ..Default::default()
        }
    }

    pub(crate) fn default_gutter_margin(font_id: FontId, font_size: Pixels, cx: &App) -> Pixels {
        -cx.text_system().descent(font_id, font_size)
    }

    /// The full width of the space taken up by the gutter.
    pub fn full_width(&self) -> Pixels {
        self.margin + self.width
    }
}

impl EditorSnapshot {
    pub fn remote_selections_in_range<'a>(
        &'a self,
        range: &'a Range<Anchor>,
        collaboration_hub: &dyn CollaborationHub,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = RemoteSelection> {
        let participant_names = collaboration_hub.user_names(cx);
        let participant_indices = collaboration_hub.user_participant_indices(cx);
        let collaborators_by_peer_id = collaboration_hub.collaborators(cx);
        let collaborators_by_replica_id = collaborators_by_peer_id
            .values()
            .map(|collaborator| (collaborator.replica_id, collaborator))
            .collect::<HashMap<_, _>>();
        self.buffer_snapshot()
            .selections_in_range(range, false)
            .filter_map(move |(replica_id, line_mode, cursor_shape, selection)| {
                if replica_id == ReplicaId::AGENT {
                    Some(RemoteSelection {
                        replica_id,
                        selection,
                        cursor_shape,
                        line_mode,
                        collaborator_id: CollaboratorId::Agent,
                        user_name: Some("Agent".into()),
                        color: cx.theme().players().agent(),
                    })
                } else {
                    let collaborator = collaborators_by_replica_id.get(&replica_id)?;
                    let participant_index = participant_indices.get(&collaborator.user_id).copied();
                    let user_name = participant_names.get(&collaborator.user_id).cloned();
                    Some(RemoteSelection {
                        replica_id,
                        selection,
                        cursor_shape,
                        line_mode,
                        collaborator_id: CollaboratorId::PeerId(collaborator.peer_id),
                        user_name,
                        color: if let Some(index) = participant_index {
                            cx.theme().players().color_for_participant(index.0)
                        } else {
                            cx.theme().players().absent()
                        },
                    })
                }
            })
    }

    pub fn language_at<T: ToOffset>(&self, position: T) -> Option<&Arc<Language>> {
        self.display_snapshot
            .buffer_snapshot()
            .language_at(position)
    }

    pub fn is_focused(&self) -> bool {
        self.is_focused
    }

    pub fn placeholder_text(&self) -> Option<String> {
        self.placeholder_display_snapshot
            .as_ref()
            .map(|display_map| display_map.text())
    }

    pub fn scroll_position(&self) -> gpui::Point<ScrollOffset> {
        self.scroll_anchor.scroll_position(&self.display_snapshot)
    }

    pub fn max_line_number_width(&self, style: &EditorStyle, window: &mut Window) -> Pixels {
        let digit_count = self.widest_line_number().ilog10() + 1;
        column_pixels(style, digit_count as usize, window)
    }

    pub fn gutter_dimensions(
        &self,
        font_id: FontId,
        font_size: Pixels,
        style: &EditorStyle,
        window: &mut Window,
        cx: &App,
    ) -> GutterDimensions {
        if self.show_gutter
            && let Some(ch_width) = cx.text_system().ch_width(font_id, font_size).log_err()
            && let Some(ch_advance) = cx.text_system().ch_advance(font_id, font_size).log_err()
        {
            let show_git_gutter = self.show_git_diff_gutter.unwrap_or_else(|| {
                matches!(
                    ProjectSettings::get_global(cx).git.git_gutter,
                    GitGutterSetting::TrackedFiles
                )
            });
            let gutter_settings = EditorSettings::get_global(cx).gutter;
            let show_line_numbers = self
                .show_line_numbers
                .unwrap_or(gutter_settings.line_numbers);
            let line_gutter_width = if show_line_numbers {
                // Avoid flicker-like gutter resizes when the line number gains another digit by
                // only resizing the gutter on files with > 10**min_line_number_digits lines.
                let min_width_for_number_on_gutter =
                    ch_advance * gutter_settings.min_line_number_digits as f32;
                self.max_line_number_width(style, window)
                    .max(min_width_for_number_on_gutter)
            } else {
                0.0.into()
            };

            let show_runnables = self.show_runnables.unwrap_or(gutter_settings.runnables);
            let show_breakpoints = self.show_breakpoints.unwrap_or(gutter_settings.breakpoints);
            let show_bookmarks = self.show_bookmarks.unwrap_or(gutter_settings.bookmarks);

            let git_blame_entries_width =
                self.git_blame_gutter_max_author_length
                    .map(|max_author_length| {
                        let renderer = cx.global::<GlobalBlameRenderer>().0.clone();
                        pub(crate) const MAX_RELATIVE_TIMESTAMP: &str = "2 years, 11 months ago";

                        /// The number of characters to dedicate to gaps and margins.
                        pub(crate) const SPACING_WIDTH: usize = 4;

                        let max_char_count = max_author_length.min(renderer.max_author_length())
                            + ::git::SHORT_SHA_LENGTH
                            + MAX_RELATIVE_TIMESTAMP.len()
                            + SPACING_WIDTH;

                        ch_advance * max_char_count
                    });

            let is_singleton = self.buffer_snapshot().is_singleton();

            let left_padding = git_blame_entries_width.unwrap_or(Pixels::ZERO)
                + if !is_singleton {
                    ch_width * 4.0
                // runnables, breakpoints and bookmarks are shown in the same place
                // if all three are there only the runnable is shown
                } else if show_runnables || show_breakpoints || show_bookmarks {
                    ch_width * 3.0
                } else if show_git_gutter && show_line_numbers {
                    ch_width * 2.0
                } else if show_git_gutter || show_line_numbers {
                    ch_width
                } else {
                    px(0.)
                };

            let shows_folds = is_singleton && gutter_settings.folds;

            let right_padding = if shows_folds && show_line_numbers {
                ch_width * 4.0
            } else if shows_folds || (!is_singleton && show_line_numbers) {
                ch_width * 3.0
            } else if show_line_numbers {
                ch_width
            } else {
                px(0.)
            };

            GutterDimensions {
                left_padding,
                right_padding,
                width: line_gutter_width + left_padding + right_padding,
                margin: GutterDimensions::default_gutter_margin(font_id, font_size, cx),
                git_blame_entries_width,
            }
        } else if self.offset_content {
            GutterDimensions::default_with_margin(font_id, font_size, cx)
        } else {
            GutterDimensions::default()
        }
    }

    /// Returns the line delta from `base` to `line` in the multibuffer, ignoring wrapped lines.
    ///
    /// This is positive if `base` is before `line`.
    pub(crate) fn relative_line_delta(
        &self,
        current_selection_head: DisplayRow,
        first_visible_row: DisplayRow,
        consider_wrapped_lines: bool,
    ) -> i64 {
        let current_selection_head = current_selection_head.as_display_point().to_point(self);
        let first_visible_row = first_visible_row.as_display_point().to_point(self);

        if consider_wrapped_lines {
            let wrap_snapshot = self.wrap_snapshot();
            let base_wrap_row = wrap_snapshot
                .make_wrap_point(current_selection_head, Bias::Left)
                .row();
            let wrap_row = wrap_snapshot
                .make_wrap_point(first_visible_row, Bias::Left)
                .row();

            wrap_row.0 as i64 - base_wrap_row.0 as i64
        } else {
            let fold_snapshot = self.fold_snapshot();
            let base_fold_row = fold_snapshot
                .to_fold_point(self.to_inlay_point(current_selection_head), Bias::Left)
                .row();
            let fold_row = fold_snapshot
                .to_fold_point(self.to_inlay_point(first_visible_row), Bias::Left)
                .row();

            fold_row as i64 - base_fold_row as i64
        }
    }

    /// Returns the unsigned relative line number to display for each row in `rows`.
    ///
    /// Wrapped rows are excluded from the hashmap if `count_relative_lines` is `false`.
    pub fn calculate_relative_line_numbers(
        &self,
        rows: &Range<DisplayRow>,
        current_selection_head: DisplayRow,
        count_wrapped_lines: bool,
    ) -> HashMap<DisplayRow, u32> {
        let initial_offset =
            self.relative_line_delta(current_selection_head, rows.start, count_wrapped_lines);

        self.row_infos(rows.start)
            .take(rows.len())
            .enumerate()
            .map(|(i, row_info)| (DisplayRow(rows.start.0 + i as u32), row_info))
            .filter(|(_row, row_info)| {
                row_info.buffer_row.is_some()
                    || (count_wrapped_lines && row_info.wrapped_buffer_row.is_some())
            })
            .enumerate()
            .filter_map(|(i, (row, row_info))| {
                // We want to ensure here that the current line has absolute
                // numbering, even if we are in a soft-wrapped line. With the
                // exception that if we are in a deleted line, we should number this
                // relative with 0, as otherwise it would have no line number at all
                let relative_line_number = (initial_offset + i as i64).unsigned_abs() as u32;

                (relative_line_number != 0
                    || row_info
                        .diff_status
                        .is_some_and(|status| status.is_deleted()))
                .then_some((row, relative_line_number))
            })
            .collect()
    }
}

pub fn column_pixels(style: &EditorStyle, column: usize, window: &Window) -> Pixels {
    let font_size = style.text.font_size.to_pixels(window.rem_size());
    let layout = window.text_system().shape_line(
        SharedString::from(" ".repeat(column)),
        font_size,
        &[TextRun {
            len: column,
            font: style.text.font(),
            color: Hsla::default(),
            ..Default::default()
        }],
        None,
    );

    layout.width
}

impl Deref for EditorSnapshot {
    type Target = DisplaySnapshot;

    fn deref(&self) -> &Self::Target {
        &self.display_snapshot
    }
}
