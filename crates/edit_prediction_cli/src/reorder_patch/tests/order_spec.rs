use super::*;
use indoc::indoc;

#[test]
fn test_parse_order_file() {
    let content = r#"
// Add new dependency
1, 49

// Add new imports and types
8-9, 51

// Add new struct and login command method
10-47

// Modify AgentServerDelegate to make status_tx optional
2-3

// Update status_tx usage to handle optional value
4
5-7

// Update all existing callers to use None for status_tx
48, 50

// Update the main login implementation to use custom command
52-55
56-95
"#;

    let order = parse_order_spec(content);

    assert_eq!(order.len(), 9);

    // First group: 1, 49
    assert_eq!(order[0], BTreeSet::from([1, 49]));

    // Second group: 8-9, 51
    assert_eq!(order[1], BTreeSet::from([8, 9, 51]));

    // Third group: 10-47
    let expected_range: BTreeSet<usize> = (10..=47).collect();
    assert_eq!(order[2], expected_range);

    // Fourth group: 2-3
    assert_eq!(order[3], BTreeSet::from([2, 3]));

    // Fifth group: 4
    assert_eq!(order[4], BTreeSet::from([4]));

    // Sixth group: 5-7
    assert_eq!(order[5], BTreeSet::from([5, 6, 7]));

    // Seventh group: 48, 50
    assert_eq!(order[6], BTreeSet::from([48, 50]));

    // Eighth group: 52-55
    assert_eq!(order[7], BTreeSet::from([52, 53, 54, 55]));

    // Ninth group: 56-95
    let expected_range_2: BTreeSet<usize> = (56..=95).collect();
    assert_eq!(order[8], expected_range_2);
}
