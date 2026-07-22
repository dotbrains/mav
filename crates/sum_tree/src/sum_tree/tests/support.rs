use super::*;
use std::cmp;

#[derive(Clone, Default, Debug)]
pub struct IntegersSummary {
    pub(super) count: usize,
    pub(super) sum: usize,
    pub(super) contains_even: bool,
    pub(super) max: u8,
}

#[derive(Ord, PartialOrd, Default, Eq, PartialEq, Clone, Debug)]
pub(super) struct Count(pub usize);

#[derive(Ord, PartialOrd, Default, Eq, PartialEq, Clone, Debug)]
pub(super) struct Sum(pub usize);

impl Item for u8 {
    type Summary = IntegersSummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        IntegersSummary {
            count: 1,
            sum: *self as usize,
            contains_even: (*self & 1) == 0,
            max: *self,
        }
    }
}

impl KeyedItem for u8 {
    type Key = u8;

    fn key(&self) -> Self::Key {
        *self
    }
}

impl ContextLessSummary for IntegersSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, other: &Self) {
        self.count += other.count;
        self.sum += other.sum;
        self.contains_even |= other.contains_even;
        self.max = cmp::max(self.max, other.max);
    }
}

impl Dimension<'_, IntegersSummary> for u8 {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &IntegersSummary, _: ()) {
        *self = summary.max;
    }
}

impl Dimension<'_, IntegersSummary> for Count {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &IntegersSummary, _: ()) {
        self.0 += summary.count;
    }
}

impl SeekTarget<'_, IntegersSummary, IntegersSummary> for Count {
    fn cmp(&self, cursor_location: &IntegersSummary, _: ()) -> Ordering {
        self.0.cmp(&cursor_location.count)
    }
}

impl Dimension<'_, IntegersSummary> for Sum {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &IntegersSummary, _: ()) {
        self.0 += summary.sum;
    }
}
