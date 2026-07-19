use super::*;

impl<T: SearchableItem> SearchableItemHandle for Entity<T> {
    fn downgrade(&self) -> Box<dyn WeakSearchableItemHandle> {
        Box::new(self.downgrade())
    }

    fn boxed_clone(&self) -> Box<dyn SearchableItemHandle> {
        Box::new(self.clone())
    }

    fn supported_options(&self, cx: &App) -> SearchOptions {
        self.read(cx).supported_options()
    }

    fn subscribe_to_search_events(
        &self,
        window: &mut Window,
        cx: &mut App,
        handler: Box<dyn Fn(&SearchEvent, &mut Window, &mut App) + Send>,
    ) -> Subscription {
        window.subscribe(self, cx, move |_, event: &SearchEvent, window, cx| {
            handler(event, window, cx)
        })
    }

    fn clear_matches(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.clear_matches(window, cx));
    }

    fn update_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().unwrap();
        self.update(cx, |this, cx| {
            this.update_matches(matches.as_slice(), active_match_index, token, window, cx)
        });
    }

    fn query_suggestion(
        &self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut App,
    ) -> String {
        self.update(cx, |this, cx| {
            this.query_suggestion(seed_query_override, window, cx)
        })
    }

    fn activate_match(
        &self,
        index: usize,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().unwrap();
        self.update(cx, |this, cx| {
            this.activate_match(index, matches.as_slice(), token, window, cx)
        });
    }

    fn select_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().unwrap();
        self.update(cx, |this, cx| {
            this.select_matches(matches.as_slice(), token, window, cx)
        });
    }

    fn match_index_for_direction(
        &self,
        matches: &AnyVec<dyn Send>,
        current_index: usize,
        direction: Direction,
        count: usize,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> usize {
        let matches = matches.downcast_ref().unwrap();
        self.update(cx, |this, cx| {
            this.match_index_for_direction(
                matches.as_slice(),
                current_index,
                direction,
                count,
                token,
                window,
                cx,
            )
        })
    }

    fn find_matches(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyVec<dyn Send>> {
        let matches = self.update(cx, |this, cx| this.find_matches(query, window, cx));
        window.spawn(cx, async |_| {
            let matches = matches.await;
            let mut any_matches = AnyVec::with_capacity::<T::Match>(matches.len());
            {
                let mut any_matches = any_matches.downcast_mut::<T::Match>().unwrap();
                for mat in matches {
                    any_matches.push(mat);
                }
            }
            any_matches
        })
    }

    fn find_matches_with_token(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<(AnyVec<dyn Send>, SearchToken)> {
        let matches_with_token = self.update(cx, |this, cx| {
            this.find_matches_with_token(query, window, cx)
        });
        window.spawn(cx, async |_| {
            let (matches, token) = matches_with_token.await;
            let mut any_matches = AnyVec::with_capacity::<T::Match>(matches.len());
            {
                let mut any_matches = any_matches.downcast_mut::<T::Match>().unwrap();
                for mat in matches {
                    any_matches.push(mat);
                }
            }
            (any_matches, token)
        })
    }

    fn active_match_index(
        &self,
        direction: Direction,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        let matches = matches.downcast_ref()?;
        self.update(cx, |this, cx| {
            this.active_match_index(direction, matches.as_slice(), token, window, cx)
        })
    }

    fn replace(
        &self,
        mat: any_vec::element::ElementRef<'_, dyn Send>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mat = mat.downcast_ref().unwrap();
        self.update(cx, |this, cx| this.replace(mat, query, token, window, cx))
    }

    fn replace_all(
        &self,
        matches: &mut dyn Iterator<Item = any_vec::element::ElementRef<'_, dyn Send>>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |this, cx| {
            this.replace_all(
                &mut matches.map(|m| m.downcast_ref().unwrap()),
                query,
                token,
                window,
                cx,
            );
        })
    }

    fn search_bar_visibility_changed(&self, visible: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.search_bar_visibility_changed(visible, window, cx)
        });
    }

    fn toggle_filtered_search_ranges(
        &mut self,
        enabled: Option<FilteredSearchRange>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |this, cx| {
            this.toggle_filtered_search_ranges(enabled, window, cx)
        });
    }

    fn set_search_is_case_sensitive(&self, enabled: Option<bool>, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.set_search_is_case_sensitive(enabled, cx)
        });
    }
}
