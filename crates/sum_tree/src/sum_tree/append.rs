use super::*;

impl<T: Item> SumTree<T> {
    pub fn extend<I>(&mut self, iter: I, cx: <T::Summary as Summary>::Context<'_>)
    where
        I: IntoIterator<Item = T>,
    {
        self.append(Self::from_iter(iter, cx), cx);
    }

    pub fn par_extend<I, Iter>(&mut self, iter: I, cx: <T::Summary as Summary>::Context<'_>)
    where
        I: IntoParallelIterator<Iter = Iter>,
        Iter: IndexedParallelIterator<Item = T>,
        T: Send + Sync,
        T::Summary: Send + Sync,
        for<'a> <T::Summary as Summary>::Context<'a>: Sync,
    {
        self.append(Self::from_par_iter(iter, cx), cx);
    }

    pub fn push(&mut self, item: T, cx: <T::Summary as Summary>::Context<'_>) {
        let summary = item.summary(cx);
        self.append(
            SumTree(Arc::new(Node::Leaf {
                summary: summary.clone(),
                items: ArrayVec::from_iter(Some(item)),
                item_summaries: ArrayVec::from_iter(Some(summary)),
            })),
            cx,
        );
    }

    pub fn append(&mut self, mut other: Self, cx: <T::Summary as Summary>::Context<'_>) {
        if self.is_empty() {
            *self = other;
        } else if !other.0.is_leaf() || !other.0.items().is_empty() {
            if self.0.height() < other.0.height() {
                if let Some(tree) = Self::append_large(self.clone(), &mut other, cx) {
                    *self = Self::from_child_trees(tree, other, cx);
                } else {
                    *self = other;
                }
            } else if let Some(split_tree) = self.push_tree_recursive(other, cx) {
                *self = Self::from_child_trees(self.clone(), split_tree, cx);
            }
        }
    }

    pub(crate) fn push_tree_recursive(
        &mut self,
        other: SumTree<T>,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Option<SumTree<T>> {
        match Arc::make_mut(&mut self.0) {
            Node::Internal {
                height,
                summary,
                child_summaries,
                child_trees,
                ..
            } => {
                let other_node = other.0.clone();
                <T::Summary as Summary>::add_summary(summary, other_node.summary(), cx);

                let height_delta = *height - other_node.height();
                let mut summaries_to_append = ArrayVec::<T::Summary, { 2 * TREE_BASE }, u8>::new();
                let mut trees_to_append = ArrayVec::<SumTree<T>, { 2 * TREE_BASE }, u8>::new();
                if height_delta == 0 {
                    summaries_to_append.extend(other_node.child_summaries().iter().cloned());
                    trees_to_append.extend(other_node.child_trees().iter().cloned());
                } else if height_delta == 1 && !other_node.is_underflowing() {
                    summaries_to_append
                        .push(other_node.summary().clone())
                        .unwrap_oob();
                    trees_to_append.push(other).unwrap_oob();
                } else {
                    let tree_to_append = child_trees
                        .last_mut()
                        .unwrap()
                        .push_tree_recursive(other, cx);
                    *child_summaries.last_mut().unwrap() =
                        child_trees.last().unwrap().0.summary().clone();

                    if let Some(split_tree) = tree_to_append {
                        summaries_to_append
                            .push(split_tree.0.summary().clone())
                            .unwrap_oob();
                        trees_to_append.push(split_tree).unwrap_oob();
                    }
                }

                let child_count = child_trees.len() + trees_to_append.len();
                if child_count > 2 * TREE_BASE {
                    let left_summaries: ArrayVec<_, { 2 * TREE_BASE }, u8>;
                    let right_summaries: ArrayVec<_, { 2 * TREE_BASE }, u8>;
                    let left_trees;
                    let right_trees;

                    let midpoint = (child_count + child_count % 2) / 2;
                    {
                        let mut all_summaries = child_summaries
                            .iter()
                            .chain(summaries_to_append.iter())
                            .cloned();
                        left_summaries = all_summaries.by_ref().take(midpoint).collect();
                        right_summaries = all_summaries.collect();
                        let mut all_trees =
                            child_trees.iter().chain(trees_to_append.iter()).cloned();
                        left_trees = all_trees.by_ref().take(midpoint).collect();
                        right_trees = all_trees.collect();
                    }
                    *summary = sum(left_summaries.iter(), cx);
                    *child_summaries = left_summaries;
                    *child_trees = left_trees;

                    Some(SumTree(Arc::new(Node::Internal {
                        height: *height,
                        summary: sum(right_summaries.iter(), cx),
                        child_summaries: right_summaries,
                        child_trees: right_trees,
                    })))
                } else {
                    child_summaries.extend(summaries_to_append);
                    child_trees.extend(trees_to_append);
                    None
                }
            }
            Node::Leaf {
                summary,
                items,
                item_summaries,
            } => {
                let other_node = other.0;

                let child_count = items.len() + other_node.items().len();
                if child_count > 2 * TREE_BASE {
                    let left_items;
                    let right_items;
                    let left_summaries;
                    let right_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8>;

                    let midpoint = (child_count + child_count % 2) / 2;
                    {
                        let mut all_items = items.iter().chain(other_node.items().iter()).cloned();
                        left_items = all_items.by_ref().take(midpoint).collect();
                        right_items = all_items.collect();

                        let mut all_summaries = item_summaries
                            .iter()
                            .chain(other_node.child_summaries())
                            .cloned();
                        left_summaries = all_summaries.by_ref().take(midpoint).collect();
                        right_summaries = all_summaries.collect();
                    }
                    *items = left_items;
                    *item_summaries = left_summaries;
                    *summary = sum(item_summaries.iter(), cx);
                    Some(SumTree(Arc::new(Node::Leaf {
                        items: right_items,
                        summary: sum(right_summaries.iter(), cx),
                        item_summaries: right_summaries,
                    })))
                } else {
                    <T::Summary as Summary>::add_summary(summary, other_node.summary(), cx);
                    items.extend(other_node.items().iter().cloned());
                    item_summaries.extend(other_node.child_summaries().iter().cloned());
                    None
                }
            }
        }
    }

    // appends the `large` tree to a `small` tree, assumes small.height() <= large.height()
    pub(crate) fn append_large(
        small: Self,
        large: &mut Self,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Option<Self> {
        if small.0.height() == large.0.height() {
            if !small.0.is_underflowing() {
                Some(small)
            } else {
                Self::merge_into_right(small, large, cx)
            }
        } else {
            debug_assert!(small.0.height() < large.0.height());
            let Node::Internal {
                height,
                summary,
                child_summaries,
                child_trees,
            } = Arc::make_mut(&mut large.0)
            else {
                unreachable!();
            };
            let mut full_summary = small.summary().clone();
            Summary::add_summary(&mut full_summary, summary, cx);
            *summary = full_summary;

            let first = child_trees.first_mut().unwrap();
            let res = Self::append_large(small, first, cx);
            *child_summaries.first_mut().unwrap() = first.summary().clone();
            if let Some(tree) = res {
                if child_trees.len() < 2 * TREE_BASE {
                    child_summaries
                        .insert(0, tree.summary().clone())
                        .unwrap_oob();
                    child_trees.insert(0, tree).unwrap_oob();
                    None
                } else {
                    let new_child_summaries = {
                        let mut res = ArrayVec::from_iter([tree.summary().clone()]);
                        res.extend(child_summaries.drain(..TREE_BASE));
                        res
                    };
                    let tree = SumTree(Arc::new(Node::Internal {
                        height: *height,
                        summary: sum(new_child_summaries.iter(), cx),
                        child_summaries: new_child_summaries,
                        child_trees: {
                            let mut res = ArrayVec::from_iter([tree]);
                            res.extend(child_trees.drain(..TREE_BASE));
                            res
                        },
                    }));

                    *summary = sum(child_summaries.iter(), cx);
                    Some(tree)
                }
            } else {
                None
            }
        }
    }

    // Merge two nodes into `large`.
    //
    // `large` will contain the contents of `small` followed by its own data.
    // If the combined data exceed the node capacity, returns a new node that
    // holds the first half of the merged items and `large` is left with the
    // second half
    //
    // The nodes must be on the same height
    // It only makes sense to call this when `small` is underflowing
    pub(crate) fn merge_into_right(
        small: Self,
        large: &mut Self,
        cx: <<T as Item>::Summary as Summary>::Context<'_>,
    ) -> Option<SumTree<T>> {
        debug_assert_eq!(small.0.height(), large.0.height());
        match (small.0.as_ref(), Arc::make_mut(&mut large.0)) {
            (
                Node::Internal {
                    summary: small_summary,
                    child_summaries: small_child_summaries,
                    child_trees: small_child_trees,
                    ..
                },
                Node::Internal {
                    summary,
                    child_summaries,
                    child_trees,
                    height,
                },
            ) => {
                let total_child_count = child_trees.len() + small_child_trees.len();
                if total_child_count <= 2 * TREE_BASE {
                    let mut all_trees = small_child_trees.clone();
                    all_trees.extend(child_trees.drain(..));
                    *child_trees = all_trees;

                    let mut all_summaries = small_child_summaries.clone();
                    all_summaries.extend(child_summaries.drain(..));
                    *child_summaries = all_summaries;

                    let mut full_summary = small_summary.clone();
                    Summary::add_summary(&mut full_summary, summary, cx);
                    *summary = full_summary;
                    None
                } else {
                    let midpoint = total_child_count.div_ceil(2);
                    let mut all_trees = small_child_trees.iter().chain(child_trees.iter()).cloned();
                    let left_trees = all_trees.by_ref().take(midpoint).collect();
                    *child_trees = all_trees.collect();

                    let mut all_summaries = small_child_summaries
                        .iter()
                        .chain(child_summaries.iter())
                        .cloned();
                    let left_summaries: ArrayVec<_, { 2 * TREE_BASE }, u8> =
                        all_summaries.by_ref().take(midpoint).collect();
                    *child_summaries = all_summaries.collect();

                    *summary = sum(child_summaries.iter(), cx);
                    Some(SumTree(Arc::new(Node::Internal {
                        height: *height,
                        summary: sum(left_summaries.iter(), cx),
                        child_summaries: left_summaries,
                        child_trees: left_trees,
                    })))
                }
            }
            (
                Node::Leaf {
                    summary: small_summary,
                    items: small_items,
                    item_summaries: small_item_summaries,
                },
                Node::Leaf {
                    summary,
                    items,
                    item_summaries,
                },
            ) => {
                let total_child_count = small_items.len() + items.len();
                if total_child_count <= 2 * TREE_BASE {
                    let mut all_items = small_items.clone();
                    all_items.extend(items.drain(..));
                    *items = all_items;

                    let mut all_summaries = small_item_summaries.clone();
                    all_summaries.extend(item_summaries.drain(..));
                    *item_summaries = all_summaries;

                    let mut full_summary = small_summary.clone();
                    Summary::add_summary(&mut full_summary, summary, cx);
                    *summary = full_summary;
                    None
                } else {
                    let midpoint = total_child_count.div_ceil(2);
                    let mut all_items = small_items.iter().chain(items.iter()).cloned();
                    let left_items = all_items.by_ref().take(midpoint).collect();
                    *items = all_items.collect();

                    let mut all_summaries = small_item_summaries
                        .iter()
                        .chain(item_summaries.iter())
                        .cloned();
                    let left_summaries: ArrayVec<_, { 2 * TREE_BASE }, u8> =
                        all_summaries.by_ref().take(midpoint).collect();
                    *item_summaries = all_summaries.collect();

                    *summary = sum(item_summaries.iter(), cx);
                    Some(SumTree(Arc::new(Node::Leaf {
                        items: left_items,
                        summary: sum(left_summaries.iter(), cx),
                        item_summaries: left_summaries,
                    })))
                }
            }
            _ => unreachable!(),
        }
    }

    pub(crate) fn from_child_trees(
        left: SumTree<T>,
        right: SumTree<T>,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Self {
        let height = left.0.height() + 1;
        let mut child_summaries = ArrayVec::new();
        child_summaries.push(left.0.summary().clone()).unwrap_oob();
        child_summaries.push(right.0.summary().clone()).unwrap_oob();
        let mut child_trees = ArrayVec::new();
        child_trees.push(left).unwrap_oob();
        child_trees.push(right).unwrap_oob();
        SumTree(Arc::new(Node::Internal {
            height,
            summary: sum(child_summaries.iter(), cx),
            child_summaries,
            child_trees,
        }))
    }

    pub(crate) fn leftmost_leaf(&self) -> &Self {
        match *self.0 {
            Node::Leaf { .. } => self,
            Node::Internal {
                ref child_trees, ..
            } => child_trees.first().unwrap().leftmost_leaf(),
        }
    }

    pub(crate) fn rightmost_leaf(&self) -> &Self {
        match *self.0 {
            Node::Leaf { .. } => self,
            Node::Internal {
                ref child_trees, ..
            } => child_trees.last().unwrap().rightmost_leaf(),
        }
    }
}
