use super::*;

impl ::sum_tree::Item for ListItem {
    type Summary = ListItemSummary;

    fn summary(&self, _: ()) -> Self::Summary {
        match self {
            ListItem::Unmeasured {
                size_hint,
                focus_handle,
            } => ListItemSummary {
                count: 1,
                rendered_count: 0,
                unrendered_count: 1,
                height: if let Some(size) = size_hint {
                    size.height
                } else {
                    px(0.)
                },
                has_focus_handles: focus_handle.is_some(),
                has_unknown_height: size_hint.is_none(),
            },
            ListItem::Measured {
                size, focus_handle, ..
            } => ListItemSummary {
                count: 1,
                rendered_count: 1,
                unrendered_count: 0,
                height: size.height,
                has_focus_handles: focus_handle.is_some(),
                has_unknown_height: false,
            },
        }
    }
}

impl ::sum_tree::ContextLessSummary for ListItemSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.count += summary.count;
        self.rendered_count += summary.rendered_count;
        self.unrendered_count += summary.unrendered_count;
        self.height += summary.height;
        self.has_focus_handles |= summary.has_focus_handles;
        self.has_unknown_height |= summary.has_unknown_height;
    }
}

impl<'a> ::sum_tree::Dimension<'a, ListItemSummary> for Count {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ListItemSummary, _: ()) {
        self.0 += summary.count;
    }
}

impl<'a> ::sum_tree::Dimension<'a, ListItemSummary> for Height {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ListItemSummary, _: ()) {
        self.0 += summary.height;
    }
}

impl ::sum_tree::SeekTarget<'_, ListItemSummary, ListItemSummary> for Count {
    fn cmp(&self, other: &ListItemSummary, _: ()) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.count).unwrap()
    }
}

impl ::sum_tree::SeekTarget<'_, ListItemSummary, ListItemSummary> for Height {
    fn cmp(&self, other: &ListItemSummary, _: ()) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.height).unwrap()
    }
}
