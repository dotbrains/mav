use super::*;

#[derive(Clone, Copy, Debug)]
pub struct DimensionPair<K, V> {
    pub key: K,
    pub value: Option<V>,
}

impl<K: Default, V: Default> Default for DimensionPair<K, V> {
    fn default() -> Self {
        Self {
            key: Default::default(),
            value: Some(Default::default()),
        }
    }
}

impl<K, V> cmp::Ord for DimensionPair<K, V>
where
    K: cmp::Ord,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl<K, V> cmp::PartialOrd for DimensionPair<K, V>
where
    K: cmp::PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K, V> cmp::PartialEq for DimensionPair<K, V>
where
    K: cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl<R, R2, K, V> ops::Sub for DimensionPair<K, V>
where
    K: ops::Sub<K, Output = R>,
    V: ops::Sub<V, Output = R2>,
{
    type Output = DimensionPair<R, R2>;

    fn sub(self, rhs: Self) -> Self::Output {
        DimensionPair {
            key: self.key - rhs.key,
            value: self.value.zip(rhs.value).map(|(a, b)| a - b),
        }
    }
}

impl<R, R2, K, V> ops::AddAssign<DimensionPair<R, R2>> for DimensionPair<K, V>
where
    K: ops::AddAssign<R>,
    V: ops::AddAssign<R2>,
{
    fn add_assign(&mut self, rhs: DimensionPair<R, R2>) {
        self.key += rhs.key;
        if let Some(value) = &mut self.value {
            if let Some(other_value) = rhs.value {
                *value += other_value;
            } else {
                self.value.take();
            }
        }
    }
}

impl<D> std::ops::AddAssign<DimensionPair<Point, D>> for Point {
    fn add_assign(&mut self, rhs: DimensionPair<Point, D>) {
        *self += rhs.key;
    }
}

impl<K, V> cmp::Eq for DimensionPair<K, V> where K: cmp::Eq {}

impl<'a, K, V, S> sum_tree::Dimension<'a, S> for DimensionPair<K, V>
where
    S: sum_tree::Summary,
    K: sum_tree::Dimension<'a, S>,
    V: sum_tree::Dimension<'a, S>,
{
    fn zero(cx: S::Context<'_>) -> Self {
        Self {
            key: K::zero(cx),
            value: Some(V::zero(cx)),
        }
    }

    fn add_summary(&mut self, summary: &'a S, cx: S::Context<'_>) {
        self.key.add_summary(summary, cx);
        if let Some(value) = &mut self.value {
            value.add_summary(summary, cx);
        }
    }
}

impl<K, V> TextDimension for DimensionPair<K, V>
where
    K: TextDimension,
    V: TextDimension,
{
    fn add_assign(&mut self, other: &Self) {
        self.key.add_assign(&other.key);
        if let Some(value) = &mut self.value {
            if let Some(other_value) = other.value.as_ref() {
                value.add_assign(other_value);
            } else {
                self.value.take();
            }
        }
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        Self {
            key: K::from_chunk(chunk),
            value: Some(V::from_chunk(chunk)),
        }
    }

    fn from_text_summary(summary: &TextSummary) -> Self {
        Self {
            key: K::from_text_summary(summary),
            value: Some(V::from_text_summary(summary)),
        }
    }
}
