use super::*;

impl<T: Item> SumTree<T> {
    pub fn cursor<'a, 'b, D>(
        &'a self,
        cx: <T::Summary as Summary>::Context<'b>,
    ) -> Cursor<'a, 'b, T, D>
    where
        D: Dimension<'a, T::Summary>,
    {
        Cursor::new(self, cx)
    }

    /// Note: If the summary type requires a non `()` context, then the filter cursor
    /// that is returned cannot be used with Rust's iterators.
    pub fn filter<'a, 'b, F, U>(
        &'a self,
        cx: <T::Summary as Summary>::Context<'b>,
        filter_node: F,
    ) -> FilterCursor<'a, 'b, F, T, U>
    where
        F: FnMut(&T::Summary) -> bool,
        U: Dimension<'a, T::Summary>,
    {
        FilterCursor::new(self, cx, filter_node)
    }

    #[allow(dead_code)]
    pub fn first(&self) -> Option<&T> {
        self.leftmost_leaf().0.items().first()
    }

    pub fn last(&self) -> Option<&T> {
        self.rightmost_leaf().0.items().last()
    }

    pub fn last_summary(&self) -> Option<&T::Summary> {
        self.rightmost_leaf().0.child_summaries().last()
    }

    pub fn update_last(
        &mut self,
        f: impl FnOnce(&mut T),
        cx: <T::Summary as Summary>::Context<'_>,
    ) {
        self.update_last_recursive(f, cx);
    }

    fn update_last_recursive(
        &mut self,
        f: impl FnOnce(&mut T),
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Option<T::Summary> {
        match Arc::make_mut(&mut self.0) {
            Node::Internal {
                summary,
                child_summaries,
                child_trees,
                ..
            } => {
                let last_summary = child_summaries.last_mut().unwrap();
                let last_child = child_trees.last_mut().unwrap();
                *last_summary = last_child.update_last_recursive(f, cx).unwrap();
                *summary = sum(child_summaries.iter(), cx);
                Some(summary.clone())
            }
            Node::Leaf {
                summary,
                items,
                item_summaries,
            } => {
                if let Some((item, item_summary)) = items.last_mut().zip(item_summaries.last_mut())
                {
                    (f)(item);
                    *item_summary = item.summary(cx);
                    *summary = sum(item_summaries.iter(), cx);
                    Some(summary.clone())
                } else {
                    None
                }
            }
        }
    }

    pub fn update_first(
        &mut self,
        f: impl FnOnce(&mut T),
        cx: <T::Summary as Summary>::Context<'_>,
    ) {
        self.update_first_recursive(f, cx);
    }

    fn update_first_recursive(
        &mut self,
        f: impl FnOnce(&mut T),
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Option<T::Summary> {
        match Arc::make_mut(&mut self.0) {
            Node::Internal {
                summary,
                child_summaries,
                child_trees,
                ..
            } => {
                let first_summary = child_summaries.first_mut().unwrap();
                let first_child = child_trees.first_mut().unwrap();
                *first_summary = first_child.update_first_recursive(f, cx).unwrap();
                *summary = sum(child_summaries.iter(), cx);
                Some(summary.clone())
            }
            Node::Leaf {
                summary,
                items,
                item_summaries,
            } => {
                if let Some((item, item_summary)) =
                    items.first_mut().zip(item_summaries.first_mut())
                {
                    (f)(item);
                    *item_summary = item.summary(cx);
                    *summary = sum(item_summaries.iter(), cx);
                    Some(summary.clone())
                } else {
                    None
                }
            }
        }
    }

    pub fn extent<'a, D: Dimension<'a, T::Summary>>(
        &'a self,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> D {
        let mut extent = D::zero(cx);
        match self.0.as_ref() {
            Node::Internal { summary, .. } | Node::Leaf { summary, .. } => {
                extent.add_summary(summary, cx);
            }
        }
        extent
    }

    pub fn summary(&self) -> &T::Summary {
        match self.0.as_ref() {
            Node::Internal { summary, .. } => summary,
            Node::Leaf { summary, .. } => summary,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self.0.as_ref() {
            Node::Internal { .. } => false,
            Node::Leaf { items, .. } => items.is_empty(),
        }
    }
}
