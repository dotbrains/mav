use super::*;

#[cfg(test)]
impl Editor {
    pub(crate) fn set_menu_edit_predictions_policy(&mut self, value: MenuEditPredictionsPolicy) {
        self.menu_edit_predictions_policy = value;
    }
}

pub(crate) struct MissingEditPredictionKeybindingTooltip;

impl Render for MissingEditPredictionKeybindingTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ui::tooltip_container(cx, |container, cx| {
            container
                .flex_shrink_0()
                .max_w_80()
                .min_h(rems_from_px(124.))
                .justify_between()
                .child(
                    v_flex()
                        .flex_1()
                        .text_ui_sm(cx)
                        .child(Label::new("Conflict with Accept Keybinding"))
                        .child("Your keymap currently overrides the default accept keybinding. To continue, assign one keybinding for the `editor::AcceptEditPrediction` action.")
                )
                .child(
                    h_flex()
                        .pb_1()
                        .gap_1()
                        .items_end()
                        .w_full()
                        .child(Button::new("open-keymap", "Assign Keybinding").size(ButtonSize::Compact).on_click(|_ev, window, cx| {
                            window.dispatch_action(mav_actions::OpenKeymapFile.boxed_clone(), cx)
                        }))
                        .child(Button::new("see-docs", "See Docs").size(ButtonSize::Compact).on_click(|_ev, _window, cx| {
                            cx.open_url("https://mav.dev/docs/completions#edit-predictions-missing-keybinding");
                        })),
                )
        })
    }
}

pub(crate) fn edit_prediction_fallback_text(
    edits: &[(Range<Anchor>, Arc<str>)],
    cx: &App,
) -> HighlightedText {
    // Fallback for providers that don't provide edit_preview (like Copilot)
    // Just show the raw edit text with basic styling
    let mut text = String::new();
    let mut highlights = Vec::new();

    let insertion_highlight_style = HighlightStyle {
        color: Some(cx.theme().colors().text),
        ..Default::default()
    };

    for (_, edit_text) in edits {
        let start_offset = text.len();
        text.push_str(edit_text);
        let end_offset = text.len();

        if start_offset < end_offset {
            highlights.push((start_offset..end_offset, insertion_highlight_style));
        }
    }

    HighlightedText {
        text: text.into(),
        highlights,
    }
}

pub(crate) fn all_edits_insertions_or_deletions(
    edits: &Vec<(Range<Anchor>, Arc<str>)>,
    snapshot: &MultiBufferSnapshot,
) -> bool {
    let mut all_insertions = true;
    let mut all_deletions = true;

    for (range, new_text) in edits.iter() {
        let range_is_empty = range.to_offset(snapshot).is_empty();
        let text_is_empty = new_text.is_empty();

        if range_is_empty != text_is_empty {
            if range_is_empty {
                all_deletions = false;
            } else {
                all_insertions = false;
            }
        } else {
            return false;
        }

        if !all_insertions && !all_deletions {
            return false;
        }
    }
    all_insertions || all_deletions
}
