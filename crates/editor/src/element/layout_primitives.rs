use super::*;

/// Determines what kinds of highlights should be applied to a line's background.
#[derive(Clone, Copy, Default)]
pub(super) struct LineHighlightSpec {
    pub(super) selection: bool,
    pub(super) breakpoint: bool,
    pub(super) _active_stack_frame: bool,
}

pub(super) enum LineNumberStyle {
    Breakpoint,
    DiffAdded,
    DiffDeleted,
    Active,
    Inactive,
}

impl LineNumberStyle {
    pub(super) fn new(
        is_active: bool,
        is_breakpoint: bool,
        diff_status: Option<DiffHunkStatus>,
    ) -> Self {
        match (
            is_active,
            is_breakpoint,
            diff_status.map(|status| status.kind),
        ) {
            (_, true, _) => Self::Breakpoint,
            (true, _, _) => Self::Active,
            (_, _, Some(DiffHunkStatusKind::Added)) => Self::DiffAdded,
            (_, _, Some(DiffHunkStatusKind::Deleted)) => Self::DiffDeleted,
            (_, _, _) => Self::Inactive,
        }
    }

    pub(super) fn color(self, colors: &theme::ThemeColors) -> Hsla {
        match self {
            Self::Breakpoint => colors.debugger_accent,
            Self::DiffAdded => colors.version_control_added,
            Self::DiffDeleted => colors.version_control_deleted,
            Self::Active => colors.editor_active_line_number,
            Self::Inactive => colors.editor_line_number,
        }
    }
}

#[derive(Debug)]
pub(super) struct SelectionLayout {
    pub(super) head: DisplayPoint,
    pub(super) cursor_shape: CursorShape,
    pub(super) is_newest: bool,
    pub(super) is_local: bool,
    pub(super) range: Range<DisplayPoint>,
    pub(super) active_rows: Range<DisplayRow>,
    pub(super) user_name: Option<SharedString>,
}

pub(super) struct InlineBlameLayout {
    pub(super) element: AnyElement,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) buffer_id: BufferId,
    pub(super) entry: BlameEntry,
}

impl SelectionLayout {
    pub(super) fn new<T: ToPoint + ToDisplayPoint + Clone>(
        selection: Selection<T>,
        line_mode: bool,
        cursor_offset: bool,
        cursor_shape: CursorShape,
        map: &DisplaySnapshot,
        is_newest: bool,
        is_local: bool,
        user_name: Option<SharedString>,
    ) -> Self {
        let point_selection = selection.map(|p| p.to_point(map.buffer_snapshot()));
        let display_selection = point_selection.map(|p| p.to_display_point(map));
        let mut range = display_selection.range();
        let mut head = display_selection.head();
        let mut active_rows = map.prev_line_boundary(point_selection.start).1.row()
            ..map.next_line_boundary(point_selection.end).1.row();

        if line_mode {
            let point_range = map.expand_to_line(point_selection.range());
            range = point_range.start.to_display_point(map)..point_range.end.to_display_point(map);
        }

        if cursor_offset && !range.is_empty() && !selection.reversed {
            if head.column() > 0 {
                head = map.clip_point(DisplayPoint::new(head.row(), head.column() - 1), Bias::Left);
            } else if head.row().0 > 0 && head != map.max_point() {
                head = map.clip_point(
                    DisplayPoint::new(
                        head.row().previous_row(),
                        map.line_len(head.row().previous_row()),
                    ),
                    Bias::Left,
                );
                range.end = DisplayPoint::new(head.row().next_row(), 0);
                active_rows.end = head.row();
            }
        }

        Self {
            head,
            cursor_shape,
            is_newest,
            is_local,
            range,
            active_rows,
            user_name,
        }
    }
}
