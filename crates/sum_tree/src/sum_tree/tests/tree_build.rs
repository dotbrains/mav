use super::*;

#[test]
fn test_extend_and_push_tree() {
    let mut tree1 = SumTree::default();
    tree1.extend(0..20, ());

    let mut tree2 = SumTree::default();
    tree2.extend(50..100, ());

    tree1.append(tree2, ());
    assert_eq!(tree1.items(()), (0..20).chain(50..100).collect::<Vec<u8>>());
}
#[test]
fn test_from_iter() {
    assert_eq!(
        SumTree::from_iter(0u8..100u8, ()).items(()),
        (0u8..100u8).collect::<Vec<_>>()
    );

    // Ensure `from_iter` works correctly when the given iterator restarts
    // after calling `next` if `None` was already returned.
    let mut ix = 0;
    let iterator = std::iter::from_fn(|| {
        ix = (ix + 1) % 2;
        if ix == 1 { Some(1u8) } else { None }
    });
    assert_eq!(SumTree::from_iter(iterator, ()).items(()), vec![1]);
}
