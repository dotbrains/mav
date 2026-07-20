use project::git_store::*;
use text::{Buffer, BufferId, ReplicaId, ToOffset as _};
use unindent::Unindent as _;

#[test]
fn test_parse_conflicts_in_buffer() {
    // Create a buffer with conflict markers
    let test_content = r#"
            This is some text before the conflict.
            <<<<<<< HEAD
            This is our version
            =======
            This is their version
            >>>>>>> branch-name

            Another conflict:
            <<<<<<< HEAD
            Our second change
            ||||||| merged common ancestors
            Original content
            =======
            Their second change
            >>>>>>> branch-name
        "#
    .unindent();

    let buffer_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, test_content);
    let snapshot = buffer.snapshot();

    let conflict_snapshot = ConflictSet::parse(&snapshot);
    assert_eq!(conflict_snapshot.conflicts.len(), 2);

    let first = &conflict_snapshot.conflicts[0];
    assert!(first.base.is_none());
    assert_eq!(first.ours_branch_name.as_ref(), "HEAD");
    assert_eq!(first.theirs_branch_name.as_ref(), "branch-name");
    let our_text = snapshot
        .text_for_range(first.ours.clone())
        .collect::<String>();
    let their_text = snapshot
        .text_for_range(first.theirs.clone())
        .collect::<String>();
    assert_eq!(our_text, "This is our version\n");
    assert_eq!(their_text, "This is their version\n");

    let second = &conflict_snapshot.conflicts[1];
    assert!(second.base.is_some());
    assert_eq!(second.ours_branch_name.as_ref(), "HEAD");
    assert_eq!(second.theirs_branch_name.as_ref(), "branch-name");
    let our_text = snapshot
        .text_for_range(second.ours.clone())
        .collect::<String>();
    let their_text = snapshot
        .text_for_range(second.theirs.clone())
        .collect::<String>();
    let base_text = snapshot
        .text_for_range(second.base.as_ref().unwrap().clone())
        .collect::<String>();
    assert_eq!(our_text, "Our second change\n");
    assert_eq!(their_text, "Their second change\n");
    assert_eq!(base_text, "Original content\n");

    // Test conflicts_in_range
    let range = snapshot.anchor_before(0)..snapshot.anchor_before(snapshot.len());
    let conflicts_in_range = conflict_snapshot.conflicts_in_range(range, &snapshot);
    assert_eq!(conflicts_in_range.len(), 2);

    // Test with a range that includes only the first conflict
    let first_conflict_end = conflict_snapshot.conflicts[0].range.end;
    let range = snapshot.anchor_before(0)..first_conflict_end;
    let conflicts_in_range = conflict_snapshot.conflicts_in_range(range, &snapshot);
    assert_eq!(conflicts_in_range.len(), 1);

    // Test with a range that includes only the second conflict
    let second_conflict_start = conflict_snapshot.conflicts[1].range.start;
    let range = second_conflict_start..snapshot.anchor_before(snapshot.len());
    let conflicts_in_range = conflict_snapshot.conflicts_in_range(range, &snapshot);
    assert_eq!(conflicts_in_range.len(), 1);

    // Test with a range that doesn't include any conflicts
    let range = buffer.anchor_after(first_conflict_end.to_next_offset(&buffer))
        ..buffer.anchor_before(second_conflict_start.to_previous_offset(&buffer));
    let conflicts_in_range = conflict_snapshot.conflicts_in_range(range, &snapshot);
    assert_eq!(conflicts_in_range.len(), 0);
}

#[test]
fn test_nested_conflict_markers() {
    // Create a buffer with nested conflict markers
    let test_content = r#"
            This is some text before the conflict.
            <<<<<<< HEAD
            This is our version
            <<<<<<< HEAD
            This is a nested conflict marker
            =======
            This is their version in a nested conflict
            >>>>>>> branch-nested
            =======
            This is their version
            >>>>>>> branch-name
        "#
    .unindent();

    let buffer_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, test_content);
    let snapshot = buffer.snapshot();

    let conflict_snapshot = ConflictSet::parse(&snapshot);

    assert_eq!(conflict_snapshot.conflicts.len(), 1);

    // The conflict should have our version, their version, but no base
    let conflict = &conflict_snapshot.conflicts[0];
    assert!(conflict.base.is_none());
    assert_eq!(conflict.ours_branch_name.as_ref(), "HEAD");
    assert_eq!(conflict.theirs_branch_name.as_ref(), "branch-nested");

    // Check that the nested conflict was detected correctly
    let our_text = snapshot
        .text_for_range(conflict.ours.clone())
        .collect::<String>();
    assert_eq!(our_text, "This is a nested conflict marker\n");
    let their_text = snapshot
        .text_for_range(conflict.theirs.clone())
        .collect::<String>();
    assert_eq!(their_text, "This is their version in a nested conflict\n");
}

#[test]
fn test_conflict_markers_at_eof() {
    let test_content = r#"
            <<<<<<< ours
            =======
            This is their version
            >>>>>>> "#
        .unindent();
    let buffer_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, test_content);
    let snapshot = buffer.snapshot();

    let conflict_snapshot = ConflictSet::parse(&snapshot);
    assert_eq!(conflict_snapshot.conflicts.len(), 1);
    assert_eq!(
        conflict_snapshot.conflicts[0].ours_branch_name.as_ref(),
        "ours"
    );
    assert_eq!(
        conflict_snapshot.conflicts[0].theirs_branch_name.as_ref(),
        "Origin" // default branch name if there is none
    );
}

#[test]
fn test_conflicts_in_range() {
    // Create a buffer with conflict markers
    let test_content = r#"
            one
            <<<<<<< HEAD1
            two
            =======
            three
            >>>>>>> branch1
            four
            five
            <<<<<<< HEAD2
            six
            =======
            seven
            >>>>>>> branch2
            eight
            nine
            <<<<<<< HEAD3
            ten
            =======
            eleven
            >>>>>>> branch3
            twelve
            <<<<<<< HEAD4
            thirteen
            =======
            fourteen
            >>>>>>> branch4
            fifteen
        "#
    .unindent();

    let buffer_id = BufferId::new(1).unwrap();
    let buffer = Buffer::new(ReplicaId::LOCAL, buffer_id, test_content.clone());
    let snapshot = buffer.snapshot();

    let conflict_snapshot = ConflictSet::parse(&snapshot);
    assert_eq!(conflict_snapshot.conflicts.len(), 4);
    assert_eq!(
        conflict_snapshot.conflicts[0].ours_branch_name.as_ref(),
        "HEAD1"
    );
    assert_eq!(
        conflict_snapshot.conflicts[0].theirs_branch_name.as_ref(),
        "branch1"
    );
    assert_eq!(
        conflict_snapshot.conflicts[1].ours_branch_name.as_ref(),
        "HEAD2"
    );
    assert_eq!(
        conflict_snapshot.conflicts[1].theirs_branch_name.as_ref(),
        "branch2"
    );
    assert_eq!(
        conflict_snapshot.conflicts[2].ours_branch_name.as_ref(),
        "HEAD3"
    );
    assert_eq!(
        conflict_snapshot.conflicts[2].theirs_branch_name.as_ref(),
        "branch3"
    );
    assert_eq!(
        conflict_snapshot.conflicts[3].ours_branch_name.as_ref(),
        "HEAD4"
    );
    assert_eq!(
        conflict_snapshot.conflicts[3].theirs_branch_name.as_ref(),
        "branch4"
    );

    let range = test_content.find("seven").unwrap()..test_content.find("eleven").unwrap();
    let range = buffer.anchor_before(range.start)..buffer.anchor_after(range.end);
    assert_eq!(
        conflict_snapshot.conflicts_in_range(range, &snapshot),
        &conflict_snapshot.conflicts[1..=2]
    );

    let range = test_content.find("one").unwrap()..test_content.find("<<<<<<< HEAD2").unwrap();
    let range = buffer.anchor_before(range.start)..buffer.anchor_after(range.end);
    assert_eq!(
        conflict_snapshot.conflicts_in_range(range, &snapshot),
        &conflict_snapshot.conflicts[0..=1]
    );

    let range =
        test_content.find("eight").unwrap() - 1..test_content.find(">>>>>>> branch3").unwrap();
    let range = buffer.anchor_before(range.start)..buffer.anchor_after(range.end);
    assert_eq!(
        conflict_snapshot.conflicts_in_range(range, &snapshot),
        &conflict_snapshot.conflicts[1..=2]
    );

    let range = test_content.find("thirteen").unwrap() - 1..test_content.len();
    let range = buffer.anchor_before(range.start)..buffer.anchor_after(range.end);
    assert_eq!(
        conflict_snapshot.conflicts_in_range(range, &snapshot),
        &conflict_snapshot.conflicts[3..=3]
    );
}
