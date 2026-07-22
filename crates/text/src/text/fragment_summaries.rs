use super::*;

impl Fragment {
    pub(crate) fn is_visible(&self, undos: &UndoMap) -> bool {
        !undos.is_undone(self.timestamp) && self.deletions.iter().all(|d| undos.is_undone(*d))
    }

    pub(crate) fn was_visible(&self, version: &clock::Global, undos: &UndoMap) -> bool {
        (version.observed(self.timestamp) && !undos.was_undone(self.timestamp, version))
            && self
                .deletions
                .iter()
                .all(|d| !version.observed(*d) || undos.was_undone(*d, version))
    }
}

impl sum_tree::Item for Fragment {
    type Summary = FragmentSummary;

    fn summary(&self, _cx: &Option<clock::Global>) -> Self::Summary {
        let mut max_version = clock::Global::new();
        max_version.observe(self.timestamp);
        for deletion in &self.deletions {
            max_version.observe(*deletion);
        }
        max_version.join(&self.max_undos);

        let mut min_insertion_version = clock::Global::new();
        min_insertion_version.observe(self.timestamp);
        let max_insertion_version = min_insertion_version.clone();
        if self.visible {
            FragmentSummary {
                max_id: self.id.clone(),
                text: FragmentTextSummary {
                    visible: self.len as usize,
                    deleted: 0,
                },
                max_version,
                min_insertion_version,
                max_insertion_version,
            }
        } else {
            FragmentSummary {
                max_id: self.id.clone(),
                text: FragmentTextSummary {
                    visible: 0,
                    deleted: self.len as usize,
                },
                max_version,
                min_insertion_version,
                max_insertion_version,
            }
        }
    }
}

impl sum_tree::Summary for FragmentSummary {
    type Context<'a> = &'a Option<clock::Global>;

    fn zero(_cx: Self::Context<'_>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, other: &Self, _: Self::Context<'_>) {
        self.max_id.assign(&other.max_id);
        self.text.visible += &other.text.visible;
        self.text.deleted += &other.text.deleted;
        self.max_version.join(&other.max_version);
        self.min_insertion_version
            .meet(&other.min_insertion_version);
        self.max_insertion_version
            .join(&other.max_insertion_version);
    }
}

impl Default for FragmentSummary {
    fn default() -> Self {
        FragmentSummary {
            max_id: Locator::min(),
            text: FragmentTextSummary::default(),
            max_version: clock::Global::new(),
            min_insertion_version: clock::Global::new(),
            max_insertion_version: clock::Global::new(),
        }
    }
}

impl sum_tree::Item for InsertionFragment {
    type Summary = InsertionFragmentKey;

    fn summary(&self, _cx: ()) -> Self::Summary {
        InsertionFragmentKey {
            timestamp: self.timestamp,
            split_offset: self.split_offset,
        }
    }
}

impl sum_tree::KeyedItem for InsertionFragment {
    type Key = InsertionFragmentKey;

    fn key(&self) -> Self::Key {
        sum_tree::Item::summary(self, ())
    }
}

impl InsertionFragment {
    pub(crate) fn new(fragment: &Fragment) -> Self {
        Self {
            timestamp: fragment.timestamp,
            split_offset: fragment.insertion_offset,
            fragment_id: fragment.id.clone(),
        }
    }

    pub(crate) fn insert_new(fragment: &Fragment) -> sum_tree::Edit<Self> {
        sum_tree::Edit::Insert(Self::new(fragment))
    }
}

impl sum_tree::ContextLessSummary for InsertionFragmentKey {
    fn zero() -> Self {
        InsertionFragmentKey {
            timestamp: Lamport::MIN,
            split_offset: 0,
        }
    }

    fn add_summary(&mut self, summary: &Self) {
        *self = *summary;
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FullOffset(pub usize);

impl ops::AddAssign<usize> for FullOffset {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl ops::Add<usize> for FullOffset {
    type Output = Self;

    fn add(mut self, rhs: usize) -> Self::Output {
        self += rhs;
        self
    }
}

impl ops::Sub for FullOffset {
    type Output = usize;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl sum_tree::Dimension<'_, FragmentSummary> for usize {
    fn zero(_: &Option<clock::Global>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &FragmentSummary, _: &Option<clock::Global>) {
        *self += summary.text.visible;
    }
}

impl sum_tree::Dimension<'_, FragmentSummary> for FullOffset {
    fn zero(_: &Option<clock::Global>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &FragmentSummary, _: &Option<clock::Global>) {
        self.0 += summary.text.visible + summary.text.deleted;
    }
}

impl<'a> sum_tree::Dimension<'a, FragmentSummary> for Option<&'a Locator> {
    fn zero(_: &Option<clock::Global>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a FragmentSummary, _: &Option<clock::Global>) {
        *self = Some(&summary.max_id);
    }
}

impl sum_tree::SeekTarget<'_, FragmentSummary, FragmentTextSummary> for usize {
    fn cmp(
        &self,
        cursor_location: &FragmentTextSummary,
        _: &Option<clock::Global>,
    ) -> cmp::Ordering {
        Ord::cmp(self, &cursor_location.visible)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum VersionedFullOffset {
    Offset(FullOffset),
    Invalid,
}

impl VersionedFullOffset {
    pub(crate) fn full_offset(&self) -> FullOffset {
        if let Self::Offset(position) = self {
            *position
        } else {
            panic!("invalid version")
        }
    }
}

impl Default for VersionedFullOffset {
    fn default() -> Self {
        Self::Offset(Default::default())
    }
}

impl<'a> sum_tree::Dimension<'a, FragmentSummary> for VersionedFullOffset {
    fn zero(_cx: &Option<clock::Global>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a FragmentSummary, cx: &Option<clock::Global>) {
        if let Self::Offset(offset) = self {
            let version = cx.as_ref().unwrap();
            if version.observed_all(&summary.max_insertion_version) {
                *offset += summary.text.visible + summary.text.deleted;
            } else if version.observed_any(&summary.min_insertion_version) {
                *self = Self::Invalid;
            }
        }
    }
}

impl sum_tree::SeekTarget<'_, FragmentSummary, Self> for VersionedFullOffset {
    fn cmp(&self, cursor_position: &Self, _: &Option<clock::Global>) -> cmp::Ordering {
        match (self, cursor_position) {
            (Self::Offset(a), Self::Offset(b)) => Ord::cmp(a, b),
            (Self::Offset(_), Self::Invalid) => cmp::Ordering::Less,
            (Self::Invalid, _) => unreachable!(),
        }
    }
}
