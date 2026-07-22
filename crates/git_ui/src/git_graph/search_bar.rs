use super::*;

impl GitGraph {
    pub(super) fn render_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let color = cx.theme().colors();
        let query_focus_handle = self
            .search_state
            .editor
            .focus_handle(cx)
            .tab_index(1)
            .tab_stop(true);
        let search_options = {
            let mut options = SearchOptions::NONE;
            options.set(
                SearchOptions::CASE_SENSITIVE,
                self.search_state.case_sensitive,
            );
            options
        };

        h_flex()
            .key_context("GitGraphSearchBar")
            .tab_index(1)
            .tab_group()
            .tab_stop(false)
            .w_full()
            .p_1p5()
            .gap_1p5()
            .border_b_1()
            .border_color(color.border_variant)
            .child(
                h_flex()
                    .h_8()
                    .flex_1()
                    .min_w_0()
                    .px_1p5()
                    .gap_1()
                    .track_focus(&query_focus_handle)
                    .border_1()
                    .border_color(color.border_variant)
                    .rounded_md()
                    .bg(color.toolbar_background)
                    .on_action(cx.listener(Self::confirm_search))
                    .child(self.search_state.editor.clone())
                    .child(SearchOption::CaseSensitive.as_button(
                        search_options,
                        SearchSource::Buffer,
                        query_focus_handle,
                    )),
            )
            .child(
                h_flex()
                    .min_w_64()
                    .gap_1()
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("git-graph-search-prev", IconName::ChevronLeft)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .tooltip(move |_, cx| {
                                Tooltip::for_action_in(
                                    "Select Previous Match",
                                    &SelectPreviousMatch,
                                    &focus_handle,
                                    cx,
                                )
                            })
                            .map(|this| {
                                if self.search_state.matches.is_empty() {
                                    this.disabled(true)
                                } else {
                                    this.disabled(false).on_click(cx.listener(|this, _, _, cx| {
                                        this.select_previous_match(cx);
                                    }))
                                }
                            })
                    })
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("git-graph-search-next", IconName::ChevronRight)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .tooltip(move |_, cx| {
                                Tooltip::for_action_in(
                                    "Select Next Match",
                                    &SelectNextMatch,
                                    &focus_handle,
                                    cx,
                                )
                            })
                            .map(|this| {
                                if self.search_state.matches.is_empty() {
                                    this.disabled(true)
                                } else {
                                    this.disabled(false).on_click(cx.listener(|this, _, _, cx| {
                                        this.select_next_match(cx);
                                    }))
                                }
                            })
                    })
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Label::new(format!(
                                    "{}/{}",
                                    self.search_state
                                        .selected_index
                                        .map(|index| index + 1)
                                        .unwrap_or(0),
                                    self.search_state.matches.len()
                                ))
                                .size(LabelSize::Small)
                                .when(self.search_state.matches.is_empty(), |this| {
                                    this.color(Color::Disabled)
                                }),
                            )
                            .when(
                                matches!(
                                    &self.search_state.state,
                                    QueryState::Confirmed((_, task)) if !task.is_ready()
                                ),
                                |this| {
                                    this.child(
                                        Icon::new(IconName::ArrowCircle)
                                            .color(Color::Accent)
                                            .size(IconSize::Small)
                                            .with_rotate_animation(2)
                                            .into_any_element(),
                                    )
                                },
                            ),
                    ),
            )
    }
}
