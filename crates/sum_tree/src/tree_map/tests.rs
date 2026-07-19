use super::*;

#[test]
fn test_basic() {
    let mut map = TreeMap::default();
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![]);

    map.insert(3, "c");
    assert_eq!(map.get(&3), Some(&"c"));
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![(&3, &"c")]);

    map.insert(1, "a");
    assert_eq!(map.get(&1), Some(&"a"));
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![(&1, &"a"), (&3, &"c")]);

    map.insert(2, "b");
    assert_eq!(map.get(&2), Some(&"b"));
    assert_eq!(map.get(&1), Some(&"a"));
    assert_eq!(map.get(&3), Some(&"c"));
    assert_eq!(
        map.iter().collect::<Vec<_>>(),
        vec![(&1, &"a"), (&2, &"b"), (&3, &"c")]
    );

    assert_eq!(map.closest(&0), None);
    assert_eq!(map.closest(&1), Some((&1, &"a")));
    assert_eq!(map.closest(&10), Some((&3, &"c")));

    map.remove(&2);
    assert_eq!(map.get(&2), None);
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![(&1, &"a"), (&3, &"c")]);

    assert_eq!(map.closest(&2), Some((&1, &"a")));

    map.remove(&3);
    assert_eq!(map.get(&3), None);
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![(&1, &"a")]);

    map.remove(&1);
    assert_eq!(map.get(&1), None);
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![]);

    map.insert(4, "d");
    map.insert(5, "e");
    map.insert(6, "f");
    map.retain(|key, _| *key % 2 == 0);
    assert_eq!(map.iter().collect::<Vec<_>>(), vec![(&4, &"d"), (&6, &"f")]);
}

#[test]
fn test_iter_from() {
    let mut map = TreeMap::default();

    map.insert("a", 1);
    map.insert("b", 2);
    map.insert("baa", 3);
    map.insert("baaab", 4);
    map.insert("c", 5);

    let result = map
        .iter_from(&"ba")
        .take_while(|(key, _)| key.starts_with("ba"))
        .collect::<Vec<_>>();

    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|(k, _)| k == &&"baa"));
    assert!(result.iter().any(|(k, _)| k == &&"baaab"));

    let result = map
        .iter_from(&"c")
        .take_while(|(key, _)| key.starts_with("c"))
        .collect::<Vec<_>>();

    assert_eq!(result.len(), 1);
    assert!(result.iter().any(|(k, _)| k == &&"c"));
}

#[test]
fn test_insert_tree() {
    let mut map = TreeMap::default();
    map.insert("a", 1);
    map.insert("b", 2);
    map.insert("c", 3);

    let mut other = TreeMap::default();
    other.insert("a", 2);
    other.insert("b", 2);
    other.insert("d", 4);

    map.insert_tree(other);

    assert_eq!(map.iter().count(), 4);
    assert_eq!(map.get(&"a"), Some(&2));
    assert_eq!(map.get(&"b"), Some(&2));
    assert_eq!(map.get(&"c"), Some(&3));
    assert_eq!(map.get(&"d"), Some(&4));
}

#[test]
fn test_extend() {
    let mut map = TreeMap::default();
    map.insert("a", 1);
    map.insert("b", 2);
    map.insert("c", 3);
    map.extend([("a", 2), ("b", 2), ("d", 4)]);
    assert_eq!(map.iter().count(), 4);
    assert_eq!(map.get(&"a"), Some(&2));
    assert_eq!(map.get(&"b"), Some(&2));
    assert_eq!(map.get(&"c"), Some(&3));
    assert_eq!(map.get(&"d"), Some(&4));
}

#[test]
fn test_remove_between_and_path_successor() {
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    pub struct PathDescendants<'a>(&'a Path);

    impl MapSeekTarget<PathBuf> for PathDescendants<'_> {
        fn cmp_cursor(&self, key: &PathBuf) -> Ordering {
            if key.starts_with(self.0) {
                Ordering::Greater
            } else {
                self.0.cmp(key)
            }
        }
    }

    let mut map = TreeMap::default();

    map.insert(PathBuf::from("a"), 1);
    map.insert(PathBuf::from("a/a"), 1);
    map.insert(PathBuf::from("b"), 2);
    map.insert(PathBuf::from("b/a/a"), 3);
    map.insert(PathBuf::from("b/a/a/a/b"), 4);
    map.insert(PathBuf::from("c"), 5);
    map.insert(PathBuf::from("c/a"), 6);

    map.remove_range(
        &PathBuf::from("b/a"),
        &PathDescendants(&PathBuf::from("b/a")),
    );

    assert_eq!(map.get(&PathBuf::from("a")), Some(&1));
    assert_eq!(map.get(&PathBuf::from("a/a")), Some(&1));
    assert_eq!(map.get(&PathBuf::from("b")), Some(&2));
    assert_eq!(map.get(&PathBuf::from("b/a/a")), None);
    assert_eq!(map.get(&PathBuf::from("b/a/a/a/b")), None);
    assert_eq!(map.get(&PathBuf::from("c")), Some(&5));
    assert_eq!(map.get(&PathBuf::from("c/a")), Some(&6));

    map.remove_range(&PathBuf::from("c"), &PathDescendants(&PathBuf::from("c")));

    assert_eq!(map.get(&PathBuf::from("a")), Some(&1));
    assert_eq!(map.get(&PathBuf::from("a/a")), Some(&1));
    assert_eq!(map.get(&PathBuf::from("b")), Some(&2));
    assert_eq!(map.get(&PathBuf::from("c")), None);
    assert_eq!(map.get(&PathBuf::from("c/a")), None);

    map.remove_range(&PathBuf::from("a"), &PathDescendants(&PathBuf::from("a")));

    assert_eq!(map.get(&PathBuf::from("a")), None);
    assert_eq!(map.get(&PathBuf::from("a/a")), None);
    assert_eq!(map.get(&PathBuf::from("b")), Some(&2));

    map.remove_range(&PathBuf::from("b"), &PathDescendants(&PathBuf::from("b")));

    assert_eq!(map.get(&PathBuf::from("b")), None);
}
