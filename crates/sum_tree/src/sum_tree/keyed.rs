use super::*;

impl<T: Item + PartialEq> PartialEq for SumTree<T> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl<T: Item + Eq> Eq for SumTree<T> {}

impl<T: KeyedItem> SumTree<T> {
    pub fn insert_or_replace<'a, 'b>(
        &'a mut self,
        item: T,
        cx: <T::Summary as Summary>::Context<'b>,
    ) -> Option<T> {
        let mut replaced = None;
        {
            let mut cursor = self.cursor::<T::Key>(cx);
            let mut new_tree = cursor.slice(&item.key(), Bias::Left);
            if let Some(cursor_item) = cursor.item()
                && cursor_item.key() == item.key()
            {
                replaced = Some(cursor_item.clone());
                cursor.next();
            }
            new_tree.push(item, cx);
            new_tree.append(cursor.suffix(), cx);
            drop(cursor);
            *self = new_tree
        };
        replaced
    }

    pub fn remove(&mut self, key: &T::Key, cx: <T::Summary as Summary>::Context<'_>) -> Option<T> {
        let mut removed = None;
        *self = {
            let mut cursor = self.cursor::<T::Key>(cx);
            let mut new_tree = cursor.slice(key, Bias::Left);
            if let Some(item) = cursor.item()
                && item.key() == *key
            {
                removed = Some(item.clone());
                cursor.next();
            }
            new_tree.append(cursor.suffix(), cx);
            new_tree
        };
        removed
    }

    pub fn edit(
        &mut self,
        mut edits: Vec<Edit<T>>,
        cx: <T::Summary as Summary>::Context<'_>,
    ) -> Vec<T> {
        if edits.is_empty() {
            return Vec::new();
        }

        let mut removed = Vec::new();
        edits.sort_unstable_by_key(|item| item.key());

        *self = {
            let mut cursor = self.cursor::<T::Key>(cx);
            let mut new_tree = SumTree::new(cx);
            let mut buffered_items = Vec::new();

            cursor.seek(&T::Key::zero(cx), Bias::Left);
            for edit in edits {
                let new_key = edit.key();
                let mut old_item = cursor.item();

                if old_item
                    .as_ref()
                    .is_some_and(|old_item| old_item.key() < new_key)
                {
                    new_tree.extend(buffered_items.drain(..), cx);
                    let slice = cursor.slice(&new_key, Bias::Left);
                    new_tree.append(slice, cx);
                    old_item = cursor.item();
                }

                if let Some(old_item) = old_item
                    && old_item.key() == new_key
                {
                    removed.push(old_item.clone());
                    cursor.next();
                }

                match edit {
                    Edit::Insert(item) => {
                        buffered_items.push(item);
                    }
                    Edit::Remove(_) => {}
                }
            }

            new_tree.extend(buffered_items, cx);
            new_tree.append(cursor.suffix(), cx);
            new_tree
        };

        removed
    }

    pub fn get<'a>(
        &'a self,
        key: &T::Key,
        cx: <T::Summary as Summary>::Context<'a>,
    ) -> Option<&'a T> {
        if let (_, _, Some(item)) = self.find_exact::<T::Key, _>(cx, key, Bias::Left) {
            Some(item)
        } else {
            None
        }
    }
}

impl<T, S> Default for SumTree<T>
where
    T: Item<Summary = S>,
    S: for<'a> Summary<Context<'a> = ()>,
{
    fn default() -> Self {
        Self::new(())
    }
}
