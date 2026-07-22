use super::*;

#[test]
fn test_cursor() {
    // Empty tree
    let tree = SumTree::<u8>::default();
    let mut cursor = tree.cursor::<IntegersSummary>(());
    assert_eq!(
        cursor.slice(&Count(0), Bias::Right).items(()),
        Vec::<u8>::new()
    );
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 0);
    cursor.prev();
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 0);
    cursor.next();
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 0);

    // Single-element tree
    let mut tree = SumTree::<u8>::default();
    tree.extend(vec![1], ());
    let mut cursor = tree.cursor::<IntegersSummary>(());
    assert_eq!(
        cursor.slice(&Count(0), Bias::Right).items(()),
        Vec::<u8>::new()
    );
    assert_eq!(cursor.item(), Some(&1));
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 0);

    cursor.next();
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&1));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 1);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&1));
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 0);

    let mut cursor = tree.cursor::<IntegersSummary>(());
    assert_eq!(cursor.slice(&Count(1), Bias::Right).items(()), [1]);
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&1));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 1);

    cursor.seek(&Count(0), Bias::Right);
    assert_eq!(
        cursor
            .slice(&tree.extent::<Count>(()), Bias::Right)
            .items(()),
        [1]
    );
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&1));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 1);

    // Multiple-element tree
    let mut tree = SumTree::<u8>::default();
    tree.extend(vec![1, 2, 3, 4, 5, 6], ());
    let mut cursor = tree.cursor::<IntegersSummary>(());

    assert_eq!(cursor.slice(&Count(2), Bias::Right).items(()), [1, 2]);
    assert_eq!(cursor.item(), Some(&3));
    assert_eq!(cursor.prev_item(), Some(&2));
    assert_eq!(cursor.next_item(), Some(&4));
    assert_eq!(cursor.start().sum, 3);

    cursor.next();
    assert_eq!(cursor.item(), Some(&4));
    assert_eq!(cursor.prev_item(), Some(&3));
    assert_eq!(cursor.next_item(), Some(&5));
    assert_eq!(cursor.start().sum, 6);

    cursor.next();
    assert_eq!(cursor.item(), Some(&5));
    assert_eq!(cursor.prev_item(), Some(&4));
    assert_eq!(cursor.next_item(), Some(&6));
    assert_eq!(cursor.start().sum, 10);

    cursor.next();
    assert_eq!(cursor.item(), Some(&6));
    assert_eq!(cursor.prev_item(), Some(&5));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 15);

    cursor.next();
    cursor.next();
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&6));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 21);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&6));
    assert_eq!(cursor.prev_item(), Some(&5));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 15);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&5));
    assert_eq!(cursor.prev_item(), Some(&4));
    assert_eq!(cursor.next_item(), Some(&6));
    assert_eq!(cursor.start().sum, 10);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&4));
    assert_eq!(cursor.prev_item(), Some(&3));
    assert_eq!(cursor.next_item(), Some(&5));
    assert_eq!(cursor.start().sum, 6);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&3));
    assert_eq!(cursor.prev_item(), Some(&2));
    assert_eq!(cursor.next_item(), Some(&4));
    assert_eq!(cursor.start().sum, 3);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&2));
    assert_eq!(cursor.prev_item(), Some(&1));
    assert_eq!(cursor.next_item(), Some(&3));
    assert_eq!(cursor.start().sum, 1);

    cursor.prev();
    assert_eq!(cursor.item(), Some(&1));
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), Some(&2));
    assert_eq!(cursor.start().sum, 0);

    cursor.prev();
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), Some(&1));
    assert_eq!(cursor.start().sum, 0);

    cursor.next();
    assert_eq!(cursor.item(), Some(&1));
    assert_eq!(cursor.prev_item(), None);
    assert_eq!(cursor.next_item(), Some(&2));
    assert_eq!(cursor.start().sum, 0);

    let mut cursor = tree.cursor::<IntegersSummary>(());
    assert_eq!(
        cursor
            .slice(&tree.extent::<Count>(()), Bias::Right)
            .items(()),
        tree.items(())
    );
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&6));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 21);

    cursor.seek(&Count(3), Bias::Right);
    assert_eq!(
        cursor
            .slice(&tree.extent::<Count>(()), Bias::Right)
            .items(()),
        [4, 5, 6]
    );
    assert_eq!(cursor.item(), None);
    assert_eq!(cursor.prev_item(), Some(&6));
    assert_eq!(cursor.next_item(), None);
    assert_eq!(cursor.start().sum, 21);

    // Seeking can bias left or right
    cursor.seek(&Count(1), Bias::Left);
    assert_eq!(cursor.item(), Some(&1));
    cursor.seek(&Count(1), Bias::Right);
    assert_eq!(cursor.item(), Some(&2));

    // Slicing without resetting starts from where the cursor is parked at.
    cursor.seek(&Count(1), Bias::Right);
    assert_eq!(cursor.slice(&Count(3), Bias::Right).items(()), vec![2, 3]);
    assert_eq!(cursor.slice(&Count(6), Bias::Left).items(()), vec![4, 5]);
    assert_eq!(cursor.slice(&Count(6), Bias::Right).items(()), vec![6]);
}
