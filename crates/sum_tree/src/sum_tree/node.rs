use super::*;

#[derive(Clone)]
pub enum Node<T: Item> {
    Internal {
        height: u8,
        summary: T::Summary,
        child_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8>,
        child_trees: ArrayVec<SumTree<T>, { 2 * TREE_BASE }, u8>,
    },
    Leaf {
        summary: T::Summary,
        items: ArrayVec<T, { 2 * TREE_BASE }, u8>,
        item_summaries: ArrayVec<T::Summary, { 2 * TREE_BASE }, u8>,
    },
}

impl<T> fmt::Debug for Node<T>
where
    T: Item + fmt::Debug,
    T::Summary: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Internal {
                height,
                summary,
                child_summaries,
                child_trees,
            } => f
                .debug_struct("Internal")
                .field("height", height)
                .field("summary", summary)
                .field("child_summaries", child_summaries)
                .field("child_trees", child_trees)
                .finish(),
            Node::Leaf {
                summary,
                items,
                item_summaries,
            } => f
                .debug_struct("Leaf")
                .field("summary", summary)
                .field("items", items)
                .field("item_summaries", item_summaries)
                .finish(),
        }
    }
}

impl<T: Item> Node<T> {
    pub(crate) fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf { .. })
    }

    pub(crate) fn height(&self) -> u8 {
        match self {
            Node::Internal { height, .. } => *height,
            Node::Leaf { .. } => 0,
        }
    }

    pub(crate) fn summary(&self) -> &T::Summary {
        match self {
            Node::Internal { summary, .. } => summary,
            Node::Leaf { summary, .. } => summary,
        }
    }

    pub(crate) fn child_summaries(&self) -> &[T::Summary] {
        match self {
            Node::Internal {
                child_summaries, ..
            } => child_summaries.as_slice(),
            Node::Leaf { item_summaries, .. } => item_summaries.as_slice(),
        }
    }

    pub(crate) fn child_trees(&self) -> &ArrayVec<SumTree<T>, { 2 * TREE_BASE }, u8> {
        match self {
            Node::Internal { child_trees, .. } => child_trees,
            Node::Leaf { .. } => panic!("Leaf nodes have no child trees"),
        }
    }

    pub(crate) fn items(&self) -> &ArrayVec<T, { 2 * TREE_BASE }, u8> {
        match self {
            Node::Leaf { items, .. } => items,
            Node::Internal { .. } => panic!("Internal nodes have no items"),
        }
    }

    pub(crate) fn is_underflowing(&self) -> bool {
        match self {
            Node::Internal { child_trees, .. } => child_trees.len() < TREE_BASE,
            Node::Leaf { items, .. } => items.len() < TREE_BASE,
        }
    }
}

#[derive(Debug)]
pub enum Edit<T: KeyedItem> {
    Insert(T),
    Remove(T::Key),
}

impl<T: KeyedItem> Edit<T> {
    pub(crate) fn key(&self) -> T::Key {
        match self {
            Edit::Insert(item) => item.key(),
            Edit::Remove(key) => key.clone(),
        }
    }
}

pub(crate) fn sum<'a, T, I>(iter: I, cx: T::Context<'_>) -> T
where
    T: 'a + Summary,
    I: Iterator<Item = &'a T>,
{
    let mut sum = T::zero(cx);
    for value in iter {
        sum.add_summary(value, cx);
    }
    sum
}
