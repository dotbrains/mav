use super::*;

pub mod debug {
    use super::*;
    use parking_lot::Mutex;
    use std::any::TypeId;
    use std::hash::{Hash, Hasher};

    static GLOBAL_DEBUG_RANGES: Mutex<Option<GlobalDebugRanges>> = Mutex::new(None);
    pub struct GlobalDebugRanges {
        pub ranges: Vec<DebugRange>,
        key_to_occurrence_index: HashMap<Key, usize>,
        next_occurrence_index: usize,
    }

    pub struct DebugRange {
        key: Key,
        pub ranges: Vec<Range<Anchor>>,
        pub value: Arc<str>,
        pub occurrence_index: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct Key {
        type_id: TypeId,
        hash: u64,
    }

    impl GlobalDebugRanges {
        pub fn with_locked<R>(f: impl FnOnce(&mut Self) -> R) -> R {
            let mut state = GLOBAL_DEBUG_RANGES.lock();
            if state.is_none() {
                *state = Some(GlobalDebugRanges {
                    ranges: Vec::new(),
                    key_to_occurrence_index: HashMap::default(),
                    next_occurrence_index: 0,
                });
            }
            if let Some(global_debug_ranges) = state.as_mut() {
                f(global_debug_ranges)
            } else {
                unreachable!()
            }
        }

        pub fn insert<K: Hash + 'static>(
            &mut self,
            key: &K,
            ranges: Vec<Range<Anchor>>,
            value: Arc<str>,
        ) {
            let occurrence_index = *self
                .key_to_occurrence_index
                .entry(Key::new(key))
                .or_insert_with(|| {
                    let occurrence_index = self.next_occurrence_index;
                    self.next_occurrence_index += 1;
                    occurrence_index
                });
            let key = Key::new(key);
            let existing = self
                .ranges
                .iter()
                .enumerate()
                .rfind(|(_, existing)| existing.key == key);
            if let Some((existing_ix, _)) = existing {
                self.ranges.remove(existing_ix);
            }
            self.ranges.push(DebugRange {
                ranges,
                key,
                value,
                occurrence_index,
            });
        }

        pub fn remove<K: Hash + 'static>(&mut self, key: &K) {
            self.remove_impl(&Key::new(key));
        }

        fn remove_impl(&mut self, key: &Key) {
            let existing = self
                .ranges
                .iter()
                .enumerate()
                .rfind(|(_, existing)| &existing.key == key);
            if let Some((existing_ix, _)) = existing {
                self.ranges.remove(existing_ix);
            }
        }

        pub fn remove_all_with_key_type<K: 'static>(&mut self) {
            self.ranges
                .retain(|item| item.key.type_id != TypeId::of::<K>());
        }
    }

    impl Key {
        fn new<K: Hash + 'static>(key: &K) -> Self {
            let type_id = TypeId::of::<K>();
            let mut hasher = collections::FxHasher::default();
            key.hash(&mut hasher);
            Key {
                type_id,
                hash: hasher.finish(),
            }
        }
    }

    pub trait ToDebugRanges {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>>;
    }

    impl<T: ToOffset> ToDebugRanges for T {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            [self.to_offset(snapshot)].to_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset + Clone> ToDebugRanges for Range<T> {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            [self.clone()].to_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToDebugRanges for Vec<T> {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            self.as_slice().to_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToDebugRanges for Vec<Range<T>> {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            self.as_slice().to_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToDebugRanges for [T] {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            self.iter()
                .map(|item| {
                    let offset = item.to_offset(snapshot);
                    offset..offset
                })
                .collect()
        }
    }

    impl<T: ToOffset> ToDebugRanges for [Range<T>] {
        fn to_debug_ranges(&self, snapshot: &BufferSnapshot) -> Vec<Range<usize>> {
            self.iter()
                .map(|range| range.start.to_offset(snapshot)..range.end.to_offset(snapshot))
                .collect()
        }
    }
}
