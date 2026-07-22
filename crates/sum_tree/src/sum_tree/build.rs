use super::*;

impl<T: Item> SumTree<T> {
    pub fn new(cx: <T::Summary as Summary>::Context<'_>) -> Self {
        SumTree(Arc::new(Node::Leaf {
            summary: <T::Summary as Summary>::zero(cx),
            items: ArrayVec::new(),
            item_summaries: ArrayVec::new(),
        }))
    }

    /// Useful in cases where the item type has a non-trivial context type, but the zero value of the summary type doesn't depend on that context.
    pub fn from_summary(summary: T::Summary) -> Self {
        SumTree(Arc::new(Node::Leaf {
            summary,
            items: ArrayVec::new(),
            item_summaries: ArrayVec::new(),
        }))
    }

    pub fn from_item(item: T, cx: <T::Summary as Summary>::Context<'_>) -> Self {
        let mut tree = Self::new(cx);
        tree.push(item, cx);
        tree
    }

    pub fn from_iter<I: IntoIterator<Item = T>>(
        iter: I,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Self {
        let mut nodes = Vec::new();

        let mut iter = iter.into_iter().fuse().peekable();
        while iter.peek().is_some() {
            let items: ArrayVec<T, { 2 * TREE_BASE }, u8> =
                iter.by_ref().take(2 * TREE_BASE).collect();
            let item_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8> =
                items.iter().map(|item| item.summary(cx)).collect();

            let mut summary = item_summaries[0].clone();
            for item_summary in &item_summaries[1..] {
                <T::Summary as Summary>::add_summary(&mut summary, item_summary, cx);
            }

            nodes.push(SumTree(Arc::new(Node::Leaf {
                summary,
                items,
                item_summaries,
            })));
        }

        let mut parent_nodes = Vec::new();
        let mut height = 0;
        while nodes.len() > 1 {
            height += 1;
            let mut current_parent_node = None;
            for child_node in nodes.drain(..) {
                let parent_node = current_parent_node.get_or_insert_with(|| {
                    SumTree(Arc::new(Node::Internal {
                        summary: <T::Summary as Summary>::zero(cx),
                        height,
                        child_summaries: ArrayVec::new(),
                        child_trees: ArrayVec::new(),
                    }))
                });
                let Node::Internal {
                    summary,
                    child_summaries,
                    child_trees,
                    ..
                } = Arc::get_mut(&mut parent_node.0).unwrap()
                else {
                    unreachable!()
                };
                let child_summary = child_node.summary();
                <T::Summary as Summary>::add_summary(summary, child_summary, cx);
                child_summaries.push(child_summary.clone()).unwrap_oob();
                child_trees.push(child_node.clone()).unwrap_oob();

                if child_trees.len() == 2 * TREE_BASE {
                    parent_nodes.extend(current_parent_node.take());
                }
            }
            parent_nodes.extend(current_parent_node.take());
            mem::swap(&mut nodes, &mut parent_nodes);
        }

        if nodes.is_empty() {
            Self::new(cx)
        } else {
            debug_assert_eq!(nodes.len(), 1);
            nodes.pop().unwrap()
        }
    }

    pub fn from_par_iter<I, Iter>(iter: I, cx: <T::Summary as Summary>::Context<'_>) -> Self
    where
        I: IntoParallelIterator<Iter = Iter>,
        Iter: IndexedParallelIterator<Item = T>,
        T: Send + Sync,
        T::Summary: Send + Sync,
        for<'a> <T::Summary as Summary>::Context<'a>: Sync,
    {
        let mut nodes = iter
            .into_par_iter()
            .chunks(2 * TREE_BASE)
            .map(|items| {
                let items: ArrayVec<T, { 2 * TREE_BASE }, u8> = items.into_iter().collect();
                let item_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8> =
                    items.iter().map(|item| item.summary(cx)).collect();
                let mut summary = item_summaries[0].clone();
                for item_summary in &item_summaries[1..] {
                    <T::Summary as Summary>::add_summary(&mut summary, item_summary, cx);
                }
                SumTree(Arc::new(Node::Leaf {
                    summary,
                    items,
                    item_summaries,
                }))
            })
            .collect::<Vec<_>>();

        let mut height = 0;
        while nodes.len() > 1 {
            height += 1;
            nodes = nodes
                .into_par_iter()
                .chunks(2 * TREE_BASE)
                .map(|child_nodes| {
                    let child_trees: ArrayVec<SumTree<T>, { 2 * TREE_BASE }, u8> =
                        child_nodes.into_iter().collect();
                    let child_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8> = child_trees
                        .iter()
                        .map(|child_tree| child_tree.summary().clone())
                        .collect();
                    let mut summary = child_summaries[0].clone();
                    for child_summary in &child_summaries[1..] {
                        <T::Summary as Summary>::add_summary(&mut summary, child_summary, cx);
                    }
                    SumTree(Arc::new(Node::Internal {
                        height,
                        summary,
                        child_summaries,
                        child_trees,
                    }))
                })
                .collect::<Vec<_>>();
        }

        if nodes.is_empty() {
            Self::new(cx)
        } else {
            debug_assert_eq!(nodes.len(), 1);
            nodes.pop().unwrap()
        }
    }

    #[allow(unused)]
    pub fn items<'a>(&'a self, cx: <T::Summary as Summary>::Context<'a>) -> Vec<T> {
        let mut items = Vec::new();
        let mut cursor = self.cursor::<()>(cx);
        cursor.next();
        while let Some(item) = cursor.item() {
            items.push(item.clone());
            cursor.next();
        }
        items
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(self)
    }
}
