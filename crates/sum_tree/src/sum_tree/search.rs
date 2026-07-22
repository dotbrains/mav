use super::*;

impl<T: Item> SumTree<T> {
    /// A more efficient version of `Cursor::new()` + `Cursor::seek()` + `Cursor::item()`.
    ///
    /// Only returns the item that exactly has the target match.
    #[instrument(skip_all)]
    pub fn find_exact<'a, 'slf, D, Target>(
        &'slf self,
        cx: <T::Summary as Summary>::Context<'a>,
        target: &Target,
        bias: Bias,
    ) -> (D, D, Option<&'slf T>)
    where
        D: Dimension<'slf, T::Summary>,
        Target: SeekTarget<'slf, T::Summary, D>,
    {
        let tree_end = D::zero(cx).with_added_summary(self.summary(), cx);
        let comparison = target.cmp(&tree_end, cx);
        if comparison == Ordering::Greater || (comparison == Ordering::Equal && bias == Bias::Right)
        {
            return (tree_end.clone(), tree_end, None);
        }

        let mut pos = D::zero(cx);
        return match Self::find_iterate::<_, _, true>(cx, target, bias, &mut pos, self) {
            Some((item, end)) => (pos, end, Some(item)),
            None => (pos.clone(), pos, None),
        };
    }

    /// A more efficient version of `Cursor::new()` + `Cursor::seek()` + `Cursor::item()`
    #[instrument(skip_all)]
    pub fn find<'a, 'slf, D, Target>(
        &'slf self,
        cx: <T::Summary as Summary>::Context<'a>,
        target: &Target,
        bias: Bias,
    ) -> (D, D, Option<&'slf T>)
    where
        D: Dimension<'slf, T::Summary>,
        Target: SeekTarget<'slf, T::Summary, D>,
    {
        let tree_end = D::zero(cx).with_added_summary(self.summary(), cx);
        let comparison = target.cmp(&tree_end, cx);
        if comparison == Ordering::Greater || (comparison == Ordering::Equal && bias == Bias::Right)
        {
            return (tree_end.clone(), tree_end, None);
        }

        let mut pos = D::zero(cx);
        return match Self::find_iterate::<_, _, false>(cx, target, bias, &mut pos, self) {
            Some((item, end)) => (pos, end, Some(item)),
            None => (pos.clone(), pos, None),
        };
    }

    pub(crate) fn find_iterate<'tree, 'a, D, Target, const EXACT: bool>(
        cx: <T::Summary as Summary>::Context<'a>,
        target: &Target,
        bias: Bias,
        position: &mut D,
        mut this: &'tree SumTree<T>,
    ) -> Option<(&'tree T, D)>
    where
        D: Dimension<'tree, T::Summary>,
        Target: SeekTarget<'tree, T::Summary, D>,
    {
        'iterate: loop {
            match &*this.0 {
                Node::Internal {
                    child_summaries,
                    child_trees,
                    ..
                } => {
                    for (child_tree, child_summary) in child_trees.iter().zip(child_summaries) {
                        let child_end = position.clone().with_added_summary(child_summary, cx);

                        let comparison = target.cmp(&child_end, cx);
                        let target_in_child = comparison == Ordering::Less
                            || (comparison == Ordering::Equal && bias == Bias::Left);
                        if target_in_child {
                            this = child_tree;
                            continue 'iterate;
                        }
                        *position = child_end;
                    }
                }
                Node::Leaf {
                    items,
                    item_summaries,
                    ..
                } => {
                    for (item, item_summary) in items.iter().zip(item_summaries) {
                        let mut child_end = position.clone();
                        child_end.add_summary(item_summary, cx);

                        let comparison = target.cmp(&child_end, cx);
                        let entry_found = if EXACT {
                            comparison == Ordering::Equal
                        } else {
                            comparison == Ordering::Less
                                || (comparison == Ordering::Equal && bias == Bias::Left)
                        };
                        if entry_found {
                            return Some((item, child_end));
                        }

                        *position = child_end;
                    }
                }
            }
            return None;
        }
    }

    /// A more efficient version of `Cursor::new()` + `Cursor::seek()` + `Cursor::item()`
    #[instrument(skip_all)]
    pub fn find_with_prev<'a, 'slf, D, Target>(
        &'slf self,
        cx: <T::Summary as Summary>::Context<'a>,
        target: &Target,
        bias: Bias,
    ) -> (D, D, Option<(Option<&'slf T>, &'slf T)>)
    where
        D: Dimension<'slf, T::Summary>,
        Target: SeekTarget<'slf, T::Summary, D>,
    {
        let tree_end = D::zero(cx).with_added_summary(self.summary(), cx);
        let comparison = target.cmp(&tree_end, cx);
        if comparison == Ordering::Greater || (comparison == Ordering::Equal && bias == Bias::Right)
        {
            return (tree_end.clone(), tree_end, None);
        }

        let mut pos = D::zero(cx);
        return match Self::find_with_prev_iterate::<_, _, false>(cx, target, bias, &mut pos, self) {
            Some((prev, item, end)) => (pos, end, Some((prev, item))),
            None => (pos.clone(), pos, None),
        };
    }

    pub(crate) fn find_with_prev_iterate<'tree, 'a, D, Target, const EXACT: bool>(
        cx: <T::Summary as Summary>::Context<'a>,
        target: &Target,
        bias: Bias,
        position: &mut D,
        mut this: &'tree SumTree<T>,
    ) -> Option<(Option<&'tree T>, &'tree T, D)>
    where
        D: Dimension<'tree, T::Summary>,
        Target: SeekTarget<'tree, T::Summary, D>,
    {
        let mut prev = None;
        'iterate: loop {
            match &*this.0 {
                Node::Internal {
                    child_summaries,
                    child_trees,
                    ..
                } => {
                    for (child_tree, child_summary) in child_trees.iter().zip(child_summaries) {
                        let child_end = position.clone().with_added_summary(child_summary, cx);

                        let comparison = target.cmp(&child_end, cx);
                        let target_in_child = comparison == Ordering::Less
                            || (comparison == Ordering::Equal && bias == Bias::Left);
                        if target_in_child {
                            this = child_tree;
                            continue 'iterate;
                        }
                        prev = child_tree.last();
                        *position = child_end;
                    }
                }
                Node::Leaf {
                    items,
                    item_summaries,
                    ..
                } => {
                    for (item, item_summary) in items.iter().zip(item_summaries) {
                        let mut child_end = position.clone();
                        child_end.add_summary(item_summary, cx);

                        let comparison = target.cmp(&child_end, cx);
                        let entry_found = if EXACT {
                            comparison == Ordering::Equal
                        } else {
                            comparison == Ordering::Less
                                || (comparison == Ordering::Equal && bias == Bias::Left)
                        };
                        if entry_found {
                            return Some((prev, item, child_end));
                        }

                        prev = Some(item);
                        *position = child_end;
                    }
                }
            }
            return None;
        }
    }
}
