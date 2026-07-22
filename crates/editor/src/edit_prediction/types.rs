pub fn make_suggestion_styles(cx: &App) -> EditPredictionStyles {
    EditPredictionStyles {
        insertion: HighlightStyle {
            color: Some(cx.theme().status().predictive),
            ..HighlightStyle::default()
        },
        whitespace: HighlightStyle {
            background_color: Some(cx.theme().status().created_background),
            ..HighlightStyle::default()
        },
    }
}

pub(super) enum EditDisplayMode {
    TabAccept,
    DiffPopover,
    Inline,
}

pub(super) enum EditPrediction {
    Edit {
        // TODO could be a language::Anchor?
        edits: Vec<(Range<Anchor>, Arc<str>)>,
        /// Predicted cursor position as (anchor, offset_from_anchor).
        /// The anchor is in multibuffer coordinates; after applying edits,
        /// resolve the anchor and add the offset to get the final cursor position.
        cursor_position: Option<(Anchor, usize)>,
        edit_preview: Option<EditPreview>,
        display_mode: EditDisplayMode,
        snapshot: BufferSnapshot,
    },
    /// Move to a specific location in the active editor
    MoveWithin {
        target: Anchor,
        snapshot: BufferSnapshot,
    },
    /// Move to a specific location in a different editor (not the active one)
    MoveOutside {
        target: language::Anchor,
        snapshot: BufferSnapshot,
    },
}

pub(super) struct EditPredictionState {
    pub(super) inlay_ids: Vec<InlayId>,
    pub(super) completion: EditPrediction,
    pub(super) completion_id: Option<SharedString>,
    pub(super) invalidation_range: Option<Range<Anchor>>,
}

pub(super) enum EditPredictionSettings {
    Disabled,
    Enabled {
        show_in_menu: bool,
        preview_requires_modifier: bool,
    },
}

pub(super) enum MenuEditPredictionsPolicy {
    #[cfg(test)]
    Never,
    ByProvider,
}

pub(super) enum EditPredictionPreview {
    /// Modifier is not pressed
    Inactive { released_too_fast: bool },
    /// Modifier pressed
    Active {
        since: Instant,
        previous_scroll_position: Option<SharedScrollAnchor>,
    },
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) enum EditPredictionKeybindSurface {
    Inline,
    CursorPopoverCompact,
    CursorPopoverExpanded,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(super) enum EditPredictionKeybindAction {
    Accept,
    Preview,
}

pub(super) struct EditPredictionKeybindDisplay {
    #[cfg(test)]
    pub(super) accept_keystroke: Option<gpui::KeybindingKeystroke>,
    #[cfg(test)]
    pub(super) preview_keystroke: Option<gpui::KeybindingKeystroke>,
    pub(super) displayed_keystroke: Option<gpui::KeybindingKeystroke>,
    pub(super) action: EditPredictionKeybindAction,
    pub(super) missing_accept_keystroke: bool,
    pub(super) show_hold_label: bool,
}

impl EditPredictionPreview {
    pub(super) fn released_too_fast(&self) -> bool {
        match self {
            EditPredictionPreview::Inactive { released_too_fast } => *released_too_fast,
            EditPredictionPreview::Active { .. } => false,
        }
    }

    pub(super) fn set_previous_scroll_position(
        &mut self,
        scroll_position: Option<SharedScrollAnchor>,
    ) {
        if let EditPredictionPreview::Active {
            previous_scroll_position,
            ..
        } = self
        {
            *previous_scroll_position = scroll_position;
        }
    }
}

pub(super) struct RegisteredEditPredictionDelegate {
    pub(super) provider: Arc<dyn EditPredictionDelegateHandle>,
    _subscription: Subscription,
}

pub(super) fn edit_prediction_edit_text(
    current_snapshot: &BufferSnapshot,
    edits: &[(Range<Anchor>, impl AsRef<str>)],
    edit_preview: &EditPreview,
    include_deletions: bool,
    multibuffer_snapshot: &MultiBufferSnapshot,
    cx: &App,
) -> HighlightedText {
    let edits = edits
        .iter()
        .filter_map(|(anchor, text)| {
            Some((
                multibuffer_snapshot
                    .anchor_range_to_buffer_anchor_range(anchor.clone())?
                    .1,
                text,
            ))
        })
        .collect::<Vec<_>>();

    edit_preview.highlight_edits(current_snapshot, &edits, include_deletions, cx)
}
