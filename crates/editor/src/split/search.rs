use super::*;

impl SearchableItem for SplittableEditor {
    type Match = Range<Anchor>;

    fn clear_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rhs_editor.update(cx, |editor, cx| {
            editor.clear_matches(window, cx);
        });
        if let Some(lhs_editor) = self.lhs_editor() {
            lhs_editor.update(cx, |editor, cx| {
                editor.clear_matches(window, cx);
            })
        }
    }

    fn update_matches(
        &mut self,
        matches: &[Self::Match],
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.update_matches(matches, active_match_index, token, window, cx);
        });
    }

    fn search_bar_visibility_changed(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if visible {
            let side = self.focused_side();
            self.searched_side = Some(side);
            match side {
                SplitSide::Left => {
                    self.rhs_editor.update(cx, |editor, cx| {
                        editor.clear_matches(window, cx);
                    });
                }
                SplitSide::Right => {
                    if let Some(lhs) = &self.lhs {
                        lhs.editor.update(cx, |editor, cx| {
                            editor.clear_matches(window, cx);
                        });
                    }
                }
            }
        } else {
            self.searched_side = None;
        }
    }

    fn query_suggestion(
        &mut self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        self.focused_editor().update(cx, |editor, cx| {
            editor.query_suggestion(seed_query_override, window, cx)
        })
    }

    fn activate_match(
        &mut self,
        index: usize,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.activate_match(index, matches, token, window, cx);
        });
    }

    fn select_matches(
        &mut self,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.select_matches(matches, token, window, cx);
        });
    }

    fn replace(
        &mut self,
        identifier: &Self::Match,
        query: &project::search::SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.replace(identifier, query, token, window, cx);
        });
    }

    fn find_matches(
        &mut self,
        query: Arc<project::search::SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Vec<Self::Match>> {
        self.focused_editor()
            .update(cx, |editor, cx| editor.find_matches(query, window, cx))
    }

    fn find_matches_with_token(
        &mut self,
        query: Arc<project::search::SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<(Vec<Self::Match>, SearchToken)> {
        let token = self.search_token();
        let editor = self.focused_editor().downgrade();
        cx.spawn_in(window, async move |_, cx| {
            let Some(matches) = editor
                .update_in(cx, |editor, window, cx| {
                    editor.find_matches(query, window, cx)
                })
                .ok()
            else {
                return (Vec::new(), token);
            };
            (matches.await, token)
        })
    }

    fn active_match_index(
        &mut self,
        direction: workspace::searchable::Direction,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.editor_for_token(token)?.update(cx, |editor, cx| {
            editor.active_match_index(direction, matches, token, window, cx)
        })
    }
}
