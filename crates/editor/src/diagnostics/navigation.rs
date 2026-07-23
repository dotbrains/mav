use super::*;

impl Editor {
    pub fn go_to_diagnostic(
        &mut self,
        action: &GoToDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.diagnostics_enabled() {
            return;
        }

        self.go_to_diagnostic_at_cursor(Direction::Next, action.severity, window, cx);
    }

    pub fn go_to_prev_diagnostic(
        &mut self,
        action: &GoToPreviousDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.diagnostics_enabled() {
            return;
        }

        self.go_to_diagnostic_at_cursor(Direction::Prev, action.severity, window, cx);
    }

    fn diagnostics_before_cursor<'a>(
        buffer: &'a MultiBufferSnapshot,
        cursor: MultiBufferOffset,
        severity: GoToDiagnosticSeverityFilter,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'a, MultiBufferOffset>> {
        buffer
            .diagnostics_in_range(MultiBufferOffset(0)..cursor)
            .filter(move |entry| entry.range.start <= cursor)
            .filter(move |entry| severity.matches(entry.diagnostic.severity))
            .filter(|entry| entry.range.start != entry.range.end)
            .filter(|entry| !entry.diagnostic.is_unnecessary)
    }

    fn diagnostics_after_cursor<'a>(
        buffer: &'a MultiBufferSnapshot,
        cursor: MultiBufferOffset,
        severity: GoToDiagnosticSeverityFilter,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'a, MultiBufferOffset>> {
        buffer
            .diagnostics_in_range(cursor..buffer.len())
            .filter(move |entry| entry.range.start >= cursor)
            .filter(move |entry| severity.matches(entry.diagnostic.severity))
            .filter(|entry| entry.range.start != entry.range.end)
            .filter(|entry| !entry.diagnostic.is_unnecessary)
    }

    /// Attempts to expand the diagnostic at the current cursor position,
    /// updating the cursor position to the diagnostic's start point.
    ///
    /// In case there's no diagnostic at the current cursor position, this will
    /// fallback to finding the next or previous diagnostic instead, depending
    /// on the provided `direction`.
    pub fn go_to_diagnostic_at_cursor(
        &mut self,
        direction: Direction,
        severity: GoToDiagnosticSeverityFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = self.buffer.read(cx).snapshot(cx);
        let selection = self
            .selections
            .newest::<MultiBufferOffset>(&self.display_snapshot(cx));

        let before = Self::diagnostics_before_cursor(&buffer, selection.start, severity);
        let after = Self::diagnostics_after_cursor(&buffer, selection.start, severity);
        let active_group_id = match &self.active_diagnostics {
            ActiveDiagnostic::Group(group) => Some(group.group_id),
            _ => None,
        };

        let mut cursor_on_active = false;
        let mut target = None;

        for diagnostic in after.chain(before) {
            let contains_cursor = diagnostic.range.contains(&selection.start)
                || diagnostic.range.end == selection.head();

            if !contains_cursor {
                continue;
            }

            if active_group_id == Some(diagnostic.diagnostic.group_id) {
                cursor_on_active = true;
            } else if target.is_none() {
                target = Some(diagnostic);
            }
        }

        match (target, cursor_on_active) {
            (Some(diagnostic), false) => self.activate_diagnostic(&buffer, diagnostic, window, cx),
            _ => self.go_to_diagnostic_in_direction(
                &buffer, &selection, direction, severity, window, cx,
            ),
        }
    }

    fn activate_diagnostic(
        &mut self,
        buffer: &MultiBufferSnapshot,
        diagnostic: DiagnosticEntryRef<MultiBufferOffset>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let diagnostic_start = buffer.anchor_after(diagnostic.range.start);
        let Some((buffer_anchor, _)) = buffer.anchor_to_buffer_anchor(diagnostic_start) else {
            return;
        };
        let buffer_id = buffer_anchor.buffer_id;
        let snapshot = self.snapshot(window, cx);
        if snapshot.intersects_fold(diagnostic.range.start) {
            self.unfold_ranges(std::slice::from_ref(&diagnostic.range), true, false, cx);
        }
        self.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges(vec![diagnostic.range.start..diagnostic.range.start])
        });
        self.activate_diagnostics(buffer_id, diagnostic, window, cx);
        self.refresh_edit_prediction(
            true,
            false,
            EditPredictionRequestTrigger::DiagnosticNavigation,
            window,
            cx,
        );
    }

    pub fn go_to_diagnostic_in_direction(
        &mut self,
        buffer: &MultiBufferSnapshot,
        selection: &Selection<MultiBufferOffset>,
        direction: Direction,
        severity: GoToDiagnosticSeverityFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut active_group_id = None;
        if let ActiveDiagnostic::Group(active_group) = &self.active_diagnostics
            && active_group.active_range.start.to_offset(&buffer) == selection.start
        {
            active_group_id = Some(active_group.group_id);
        }

        let before = Self::diagnostics_before_cursor(&buffer, selection.start, severity);
        let after = Self::diagnostics_after_cursor(&buffer, selection.start, severity);

        let mut found: Option<DiagnosticEntryRef<MultiBufferOffset>> = None;
        if direction == Direction::Prev {
            'outer: for prev_diagnostics in [before.collect::<Vec<_>>(), after.collect::<Vec<_>>()]
            {
                for diagnostic in prev_diagnostics.into_iter().rev() {
                    if diagnostic.range.start != selection.start
                        || active_group_id
                            .is_some_and(|active| diagnostic.diagnostic.group_id < active)
                    {
                        found = Some(diagnostic);
                        break 'outer;
                    }
                }
            }
        } else {
            for diagnostic in after.chain(before) {
                if diagnostic.range.start != selection.start
                    || active_group_id.is_some_and(|active| diagnostic.diagnostic.group_id > active)
                {
                    found = Some(diagnostic);
                    break;
                }
            }
        }

        let Some(next_diagnostic) = found else {
            return;
        };

        self.activate_diagnostic(&buffer, next_diagnostic, window, cx);
    }
}
