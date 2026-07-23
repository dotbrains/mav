use super::*;

#[test]
fn test_undo_redo() {
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "1234");
    // Set group interval to zero so as to not group edits in the undo stack.
    buffer.set_group_interval(Duration::from_secs(0));

    buffer.edit([(1..1, "abx")]);
    buffer.edit([(3..4, "yzef")]);
    buffer.edit([(3..5, "cd")]);
    assert_eq!(buffer.text(), "1abcdef234");

    let entries = buffer.history.undo_stack.clone();
    assert_eq!(entries.len(), 3);

    buffer.undo_or_redo(entries[0].transaction.clone());
    assert_eq!(buffer.text(), "1cdef234");
    buffer.undo_or_redo(entries[0].transaction.clone());
    assert_eq!(buffer.text(), "1abcdef234");

    buffer.undo_or_redo(entries[1].transaction.clone());
    assert_eq!(buffer.text(), "1abcdx234");
    buffer.undo_or_redo(entries[2].transaction.clone());
    assert_eq!(buffer.text(), "1abx234");
    buffer.undo_or_redo(entries[1].transaction.clone());
    assert_eq!(buffer.text(), "1abyzef234");
    buffer.undo_or_redo(entries[2].transaction.clone());
    assert_eq!(buffer.text(), "1abcdef234");

    buffer.undo_or_redo(entries[2].transaction.clone());
    assert_eq!(buffer.text(), "1abyzef234");
    buffer.undo_or_redo(entries[0].transaction.clone());
    assert_eq!(buffer.text(), "1yzef234");
    buffer.undo_or_redo(entries[1].transaction.clone());
    assert_eq!(buffer.text(), "1234");
}

#[test]
fn test_history() {
    let mut now = Instant::now();
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "123456");
    buffer.set_group_interval(Duration::from_millis(300));

    let transaction_1 = buffer.start_transaction_at(now).unwrap();
    buffer.edit([(2..4, "cd")]);
    buffer.end_transaction_at(now);
    assert_eq!(buffer.text(), "12cd56");

    buffer.start_transaction_at(now);
    buffer.edit([(4..5, "e")]);
    buffer.end_transaction_at(now).unwrap();
    assert_eq!(buffer.text(), "12cde6");

    now += buffer.transaction_group_interval() + Duration::from_millis(1);
    buffer.start_transaction_at(now);
    buffer.edit([(0..1, "a")]);
    buffer.edit([(1..1, "b")]);
    buffer.end_transaction_at(now).unwrap();
    assert_eq!(buffer.text(), "ab2cde6");

    // Last transaction happened past the group interval, undo it on its own.
    buffer.undo();
    assert_eq!(buffer.text(), "12cde6");

    // First two transactions happened within the group interval, undo them together.
    buffer.undo();
    assert_eq!(buffer.text(), "123456");

    // Redo the first two transactions together.
    buffer.redo();
    assert_eq!(buffer.text(), "12cde6");

    // Redo the last transaction on its own.
    buffer.redo();
    assert_eq!(buffer.text(), "ab2cde6");

    buffer.start_transaction_at(now);
    assert!(buffer.end_transaction_at(now).is_none());
    buffer.undo();
    assert_eq!(buffer.text(), "12cde6");

    // Redo stack gets cleared after performing an edit.
    buffer.start_transaction_at(now);
    buffer.edit([(0..0, "X")]);
    buffer.end_transaction_at(now);
    assert_eq!(buffer.text(), "X12cde6");
    buffer.redo();
    assert_eq!(buffer.text(), "X12cde6");
    buffer.undo();
    assert_eq!(buffer.text(), "12cde6");
    buffer.undo();
    assert_eq!(buffer.text(), "123456");

    // Transactions can be grouped manually.
    buffer.redo();
    buffer.redo();
    assert_eq!(buffer.text(), "X12cde6");
    buffer.group_until_transaction(transaction_1);
    buffer.undo();
    assert_eq!(buffer.text(), "123456");
    buffer.redo();
    assert_eq!(buffer.text(), "X12cde6");
}

#[test]
fn test_finalize_last_transaction() {
    let now = Instant::now();
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "123456");
    buffer.history.group_interval = Duration::from_millis(1);

    buffer.start_transaction_at(now);
    buffer.edit([(2..4, "cd")]);
    buffer.end_transaction_at(now);
    assert_eq!(buffer.text(), "12cd56");

    buffer.finalize_last_transaction();
    buffer.start_transaction_at(now);
    buffer.edit([(4..5, "e")]);
    buffer.end_transaction_at(now).unwrap();
    assert_eq!(buffer.text(), "12cde6");

    buffer.start_transaction_at(now);
    buffer.edit([(0..1, "a")]);
    buffer.edit([(1..1, "b")]);
    buffer.end_transaction_at(now).unwrap();
    assert_eq!(buffer.text(), "ab2cde6");

    buffer.undo();
    assert_eq!(buffer.text(), "12cd56");

    buffer.undo();
    assert_eq!(buffer.text(), "123456");

    buffer.redo();
    assert_eq!(buffer.text(), "12cd56");

    buffer.redo();
    assert_eq!(buffer.text(), "ab2cde6");
}

#[test]
fn test_edited_ranges_for_transaction() {
    let now = Instant::now();
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "1234567");

    buffer.start_transaction_at(now);
    buffer.edit([(2..4, "cd")]);
    buffer.edit([(6..6, "efg")]);
    buffer.end_transaction_at(now);
    assert_eq!(buffer.text(), "12cd56efg7");

    let tx = buffer.finalize_last_transaction().unwrap().clone();
    assert_eq!(
        buffer
            .edited_ranges_for_transaction::<usize>(&tx)
            .collect::<Vec<_>>(),
        [2..4, 6..9]
    );

    buffer.edit([(5..5, "hijk")]);
    assert_eq!(buffer.text(), "12cd5hijk6efg7");
    assert_eq!(
        buffer
            .edited_ranges_for_transaction::<usize>(&tx)
            .collect::<Vec<_>>(),
        [2..4, 10..13]
    );

    buffer.edit([(4..4, "l")]);
    assert_eq!(buffer.text(), "12cdl5hijk6efg7");
    assert_eq!(
        buffer
            .edited_ranges_for_transaction::<usize>(&tx)
            .collect::<Vec<_>>(),
        [2..4, 11..14]
    );
}
