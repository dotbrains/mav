use super::*;

#[test]
fn test_edit() {
    let mut tree = SumTree::<u8>::default();

    let removed = tree.edit(vec![Edit::Insert(1), Edit::Insert(2), Edit::Insert(0)], ());
    assert_eq!(tree.items(()), vec![0, 1, 2]);
    assert_eq!(removed, Vec::<u8>::new());
    assert_eq!(tree.get(&0, ()), Some(&0));
    assert_eq!(tree.get(&1, ()), Some(&1));
    assert_eq!(tree.get(&2, ()), Some(&2));
    assert_eq!(tree.get(&4, ()), None);

    let removed = tree.edit(vec![Edit::Insert(2), Edit::Insert(4), Edit::Remove(0)], ());
    assert_eq!(tree.items(()), vec![1, 2, 4]);
    assert_eq!(removed, vec![0, 2]);
    assert_eq!(tree.get(&0, ()), None);
    assert_eq!(tree.get(&1, ()), Some(&1));
    assert_eq!(tree.get(&2, ()), Some(&2));
    assert_eq!(tree.get(&4, ()), Some(&4));
}
