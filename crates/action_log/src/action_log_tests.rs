use super::*;

use super::*;
use buffer_diff::DiffHunkStatusKind;
use gpui::TestAppContext;
use indoc::indoc;
use language::Point;
use project::{FakeFs, Fs, Project, RemoveOptions};
use rand::prelude::*;
use serde_json::json;
use settings::SettingsStore;
use std::env;
use util::{RandomCharIter, path};

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[gpui::test(iterations = 10)]
async fn test_keep_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(4, 2)..Point::new(4, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndEf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(2, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(4, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(3, 0)..Point::new(4, 3), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(2, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(0, 0)..Point::new(4, 3), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_deletions(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({"file": "abc\ndef\nghi\njkl\nmno\npqr"}),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 0)..Point::new(2, 0), "")], None, cx)
                .unwrap();
            buffer.finalize_last_transaction();
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(3, 0)..Point::new(4, 0), "")], None, cx)
                .unwrap();
            buffer.finalize_last_transaction();
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nghi\njkl\npqr"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(1, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(3, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "mno\n".into(),
                }
            ],
        )]
    );

    buffer.update(cx, |buffer, cx| buffer.undo(cx));
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nghi\njkl\nmno\npqr"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(1, 0),
                diff_status: DiffHunkStatusKind::Deleted,
                old_text: "def\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(1, 0)..Point::new(1, 0), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_overlapping_user_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 2)..Point::new(2, 3), "F\nGHI")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndeF\nGHI\njkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(0, 2)..Point::new(0, 2), "X"),
                (Point::new(3, 0)..Point::new(3, 0), "Y"),
            ],
            None,
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abXc\ndeF\nGHI\nYjkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 1)..Point::new(1, 1), "Z")], None, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abXc\ndZeF\nGHI\nYjkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(0, 0)..Point::new(1, 0), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_creating_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("lorem", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 5),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| buffer.edit([(0..0, "X")], None, cx));
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 6),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), 0..5, None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_overwriting_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "Lorem ipsum dolor"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("sit amet consecteur", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 19),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(buffer.clone(), vec![2..5], None, cx);
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert_eq!(
        buffer.read_with(cx, |buffer, _cx| buffer.text()),
        "Lorem ipsum dolor"
    );
}

#[gpui::test(iterations = 10)]
async fn test_overwriting_previously_edited_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "Lorem ipsum dolor"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.append(" sit amet consecteur", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 37),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "Lorem ipsum dolor".into(),
            }],
        )]
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("rewritten", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 9),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(buffer.clone(), vec![2..5], None, cx);
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert_eq!(
        buffer.read_with(cx, |buffer, _cx| buffer.text()),
        "Lorem ipsum dolor"
    );
}

#[gpui::test(iterations = 10)]
async fn test_deleting_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({"file1": "lorem\n", "file2": "ipsum\n"}),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let file1_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();
    let file2_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file2", cx))
        .unwrap();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let buffer1 = project
        .update(cx, |project, cx| {
            project.open_buffer(file1_path.clone(), cx)
        })
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |project, cx| {
            project.open_buffer(file2_path.clone(), cx)
        })
        .await
        .unwrap();

    action_log.update(cx, |log, cx| log.will_delete_buffer(buffer1.clone(), cx));
    action_log.update(cx, |log, cx| log.will_delete_buffer(buffer2.clone(), cx));
    project
        .update(cx, |project, cx| {
            project.delete_file(file1_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    project
        .update(cx, |project, cx| {
            project.delete_file(file2_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![
            (
                buffer1.clone(),
                vec![HunkStatus {
                    range: Point::new(0, 0)..Point::new(0, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "lorem\n".into(),
                }]
            ),
            (
                buffer2.clone(),
                vec![HunkStatus {
                    range: Point::new(0, 0)..Point::new(0, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "ipsum\n".into(),
                }],
            )
        ]
    );

    // Simulate file1 being recreated externally.
    fs.insert_file(path!("/dir/file1"), "LOREM".as_bytes().to_vec())
        .await;

    // Simulate file2 being recreated by a tool.
    let buffer2 = project
        .update(cx, |project, cx| project.open_buffer(file2_path, cx))
        .await
        .unwrap();
    action_log.update(cx, |log, cx| log.buffer_created(buffer2.clone(), cx));
    buffer2.update(cx, |buffer, cx| buffer.set_text("IPSUM", cx));
    action_log.update(cx, |log, cx| log.buffer_edited(buffer2.clone(), cx));
    project
        .update(cx, |project, cx| project.save_buffer(buffer2.clone(), cx))
        .await
        .unwrap();

    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer2.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 5),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    // Simulate file2 being deleted externally.
    fs.remove_file(path!("/dir/file2").as_ref(), RemoveOptions::default())
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E\nXYZ")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(5, 2)..Point::new(5, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    // If the rejected range doesn't overlap with any hunk, we ignore it.
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(4, 0)..Point::new(4, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(1, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(4, 0)..Point::new(4, 3),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "mno".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(4, 0)..Point::new(4, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmno"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_multiple_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E\nXYZ")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(5, 2)..Point::new(5, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log.update(cx, |log, cx| {
        let range_1 = buffer.read(cx).anchor_before(Point::new(0, 0))
            ..buffer.read(cx).anchor_before(Point::new(1, 0));
        let range_2 = buffer.read(cx).anchor_before(Point::new(5, 0))
            ..buffer.read(cx).anchor_before(Point::new(5, 3));

        let (task, _) =
            log.reject_edits_in_ranges(buffer.clone(), vec![range_1, range_2], None, cx);
        task.detach();
        assert_eq!(
            buffer.read_with(cx, |buffer, _| buffer.text()),
            "abc\ndef\nghi\njkl\nmno"
        );
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmno"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_deleted_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "content"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.will_delete_buffer(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| {
            project.delete_file(file_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(!fs.is_file(path!("/dir/file").as_ref()).await);
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 0),
                diff_status: DiffHunkStatusKind::Deleted,
                old_text: "content".into(),
            }]
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(0, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(buffer.read_with(cx, |buffer, _| buffer.text()), "content");
    assert!(fs.is_file(path!("/dir/file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_created_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("content", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 7),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(0, 11)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(!fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test]
async fn test_reject_created_file_with_user_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    cx.run_until_parked();

    // User makes additional edits
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(10..10, "\nuser added this line")], None, cx);
        });
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    // Reject all
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(100, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();

    // File should still contain all the content
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    let content = buffer.read_with(cx, |buffer, _| buffer.text());
    assert_eq!(content, "ai content\nuser added this line");
}

#[gpui::test]
async fn test_reject_after_accepting_hunk_on_created_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v1", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User accepts the single hunk
    action_log.update(cx, |log, cx| {
        let buffer_range = Anchor::min_max_range_for_buffer(buffer.read(cx).remote_id());
        log.keep_edits_in_range(buffer.clone(), buffer_range, None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    // AI modifies the file
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v2", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User rejects the hunk
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Anchor::min_max_range_for_buffer(
                    buffer.read(cx).remote_id(),
                )],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await,);
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "ai content v1"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test]
async fn test_reject_edits_on_previously_accepted_created_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v1", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    // User clicks "Accept All"
    action_log.update(cx, |log, cx| log.keep_all_edits(None, cx));
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]); // Hunks are cleared

    // AI modifies file again
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v2", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User clicks "Reject All"
    action_log
        .update(cx, |log, cx| log.reject_all_edits(None, cx))
        .await;
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "ai content v1"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 100)]
async fn test_random_diffs(mut rng: StdRng, cx: &mut TestAppContext) {
    init_test(cx);

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(20);

    let text = RandomCharIter::new(&mut rng).take(50).collect::<String>();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": text})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));

    for _ in 0..operations {
        match rng.random_range(0..100) {
            0..25 => {
                action_log.update(cx, |log, cx| {
                    let range = buffer.read(cx).random_byte_range(0, &mut rng);
                    log::info!("keeping edits in range {:?}", range);
                    log.keep_edits_in_range(buffer.clone(), range, None, cx)
                });
            }
            25..50 => {
                action_log
                    .update(cx, |log, cx| {
                        let range = buffer.read(cx).random_byte_range(0, &mut rng);
                        log::info!("rejecting edits in range {:?}", range);
                        let (task, _) =
                            log.reject_edits_in_ranges(buffer.clone(), vec![range], None, cx);
                        task
                    })
                    .await
                    .unwrap();
            }
            _ => {
                let is_agent_edit = rng.random_bool(0.5);
                if is_agent_edit {
                    log::info!("agent edit");
                } else {
                    log::info!("user edit");
                }
                cx.update(|cx| {
                    buffer.update(cx, |buffer, cx| buffer.randomly_edit(&mut rng, 1, cx));
                    if is_agent_edit {
                        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
                    }
                });
            }
        }

        if rng.random_bool(0.2) {
            quiesce(&action_log, &buffer, cx);
        }
    }

    quiesce(&action_log, &buffer, cx);

    fn quiesce(action_log: &Entity<ActionLog>, buffer: &Entity<Buffer>, cx: &mut TestAppContext) {
        log::info!("quiescing...");
        cx.run_until_parked();
        action_log.update(cx, |log, cx| {
            let tracked_buffer = log.tracked_buffers.get(buffer).unwrap();
            let mut old_text = tracked_buffer.diff_base.clone();
            let new_text = buffer.read(cx).as_rope();
            for edit in tracked_buffer.unreviewed_edits.edits() {
                let old_start = old_text.point_to_offset(Point::new(edit.new.start, 0));
                let old_end = old_text.point_to_offset(cmp::min(
                    Point::new(edit.new.start + edit.old_len(), 0),
                    old_text.max_point(),
                ));
                old_text.replace(
                    old_start..old_end,
                    &new_text.slice_rows(edit.new.clone()).to_string(),
                );
            }
            pretty_assertions::assert_eq!(old_text.to_string(), new_text.to_string());
        })
    }
}

#[gpui::test]
async fn test_keep_edits_on_commit(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.txt": "a\nb\nc\nd\ne\nf\ng\nh\ni\nj",
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.txt"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    // Edit at the very start: a -> A
                    (Point::new(0, 0)..Point::new(0, 1), "A"),
                    // Deletion in the middle: remove lines d and e
                    (Point::new(3, 0)..Point::new(5, 0), ""),
                    // Modification: g -> GGG
                    (Point::new(6, 0)..Point::new(6, 1), "GGG"),
                    // Addition: insert new line after h
                    (Point::new(7, 1)..Point::new(7, 1), "\nNEW"),
                    // Edit the very last character: j -> J
                    (Point::new(9, 0)..Point::new(9, 1), "J"),
                ],
                None,
                cx,
            );
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(0, 0)..Point::new(1, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "a\n".into()
                },
                HunkStatus {
                    range: Point::new(3, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "d\ne\n".into()
                },
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(5, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "g\n".into()
                },
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Simulate a git commit that matches some edits but not others:
    // - Accepts the first edit (a -> A)
    // - Accepts the deletion (remove d and e)
    // - Makes a different change to g (g -> G instead of GGG)
    // - Ignores the NEW line addition
    // - Ignores the last line edit (j stays as j)
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nG\nh\ni\nj".into())],
        "0000001",
    );
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(5, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "g\n".into()
                },
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Make another commit that accepts the NEW line but with different content
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nGGG\nh\nDIFFERENT\ni\nj".into())],
        "0000002",
    );
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer,
            vec![
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Final commit that accepts all remaining edits
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nGGG\nh\nNEW\ni\nJ".into())],
        "0000003",
    );
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test]
async fn test_keep_edits_on_commit_with_shifted_diff_boundaries(cx: &mut TestAppContext) {
    init_test(cx);

    let initial_text = indoc! {"
            use crate::{Alpha, Beta};

            fn keep() {
                work();
            }

            fn remove() {
                work();
            }

            fn after() {
                work();
            }
        "};
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.rs": initial_text,
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.rs", initial_text.into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.rs"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let final_text = indoc! {"
            use crate::{Alpha};

            fn keep() {
                work();
            }

            fn after() {
                work();
            }
        "};

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.set_text(final_text, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert!(!unreviewed_hunks(&action_log, cx).is_empty());

    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.rs", final_text.into())],
        "0000001",
    );
    cx.run_until_parked();

    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

/// Regression test: when head_commit updates before the BufferDiff's base
/// text does, an intermediate DiffChanged (e.g. from a buffer-edit diff
/// recalculation) must NOT consume the commit signal.  The subscription
/// should only fire once the base text itself has changed.
#[gpui::test]
async fn test_keep_edits_on_commit_with_stale_diff_changed(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.txt": "aaa\nbbb\nccc\nddd\neee",
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "aaa\nbbb\nccc\nddd\neee".into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.txt"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    // Agent makes an edit: bbb -> BBB
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(Point::new(1, 0)..Point::new(1, 3), "BBB")], None, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Verify the edit is tracked
    let hunks = unreviewed_hunks(&action_log, cx);
    assert_eq!(hunks.len(), 1);
    let hunk = &hunks[0].1;
    assert_eq!(hunk.len(), 1);
    assert_eq!(hunk[0].old_text, "bbb\n");

    // Simulate the race condition: update only the HEAD SHA first,
    // without changing the committed file contents. This is analogous
    // to compute_snapshot updating head_commit before
    // reload_buffer_diff_bases has loaded the new base text.
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.refs.insert("HEAD".into(), "0000001".into());
    })
    .unwrap();
    cx.run_until_parked();

    // Make a user edit (on a different line) to trigger a buffer diff
    // recalculation.  This fires DiffChanged while the BufferDiff base
    // text is still the OLD text.  With the old head_commit-based
    // subscription this would "consume" the commit detection.
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(Point::new(3, 0)..Point::new(3, 3), "DDD")], None, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Now update the committed file contents to match the buffer
    // (the agent edit was committed). Keep the same SHA so head_commit
    // does NOT change again — this is the second half of the race.
    {
        use git::repository::repo_path;
        fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
            state
                .head_contents
                .insert(repo_path("file.txt"), "aaa\nBBB\nccc\nDDD\neee".into());
        })
        .unwrap();
    }
    cx.run_until_parked();

    // The agent's edit (bbb -> BBB) should be accepted because the
    // committed content now matches. Only the user edit (ddd -> DDD)
    // should remain, but since the user edit is tracked as coming from
    // the user (ChangeAuthor::User) it would have been rebased into
    // the diff base already. So no unreviewed hunks should remain.
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![],
        "agent edits should have been accepted after the base text update"
    );
}

#[gpui::test]
async fn test_undo_last_reject(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "abc\ndef\nghi"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    // Track the buffer and make an agent edit
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit(
                    [(Point::new(1, 0)..Point::new(1, 3), "AGENT_EDIT")],
                    None,
                    cx,
                )
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Verify the agent edit is there
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nAGENT_EDIT\nghi"
    );
    assert!(!unreviewed_hunks(&action_log, cx).is_empty());

    // Reject all edits
    action_log
        .update(cx, |log, cx| log.reject_all_edits(None, cx))
        .await;
    cx.run_until_parked();

    // Verify the buffer is back to original
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi"
    );
    assert!(unreviewed_hunks(&action_log, cx).is_empty());

    // Verify undo state is available
    assert!(action_log.read_with(cx, |log, _| log.has_pending_undo()));

    // Undo the reject
    action_log
        .update(cx, |log, cx| log.undo_last_reject(cx))
        .await;

    cx.run_until_parked();

    // Verify the agent edit is restored
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nAGENT_EDIT\nghi"
    );

    // Verify undo state is cleared
    assert!(!action_log.read_with(cx, |log, _| log.has_pending_undo()));
}

#[gpui::test]
async fn test_linked_action_log_buffer_read(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
    });

    // Neither log considers the buffer stale immediately after reading it.
    let child_stale = cx.read(|cx| {
        child_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    let parent_stale = cx.read(|cx| {
        parent_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    assert!(child_stale.is_empty());
    assert!(parent_stale.is_empty());

    // Simulate a user edit after the agent read the file.
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(0..5, "goodbye")], None, cx).unwrap();
        });
    });
    cx.run_until_parked();

    // Both child and parent should see the buffer as stale because both tracked
    // it at the pre-edit version via buffer_read forwarding.
    let child_stale = cx.read(|cx| {
        child_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    let parent_stale = cx.read(|cx| {
        parent_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(child_stale, vec![buffer.clone()]);
    assert_eq!(parent_stale, vec![buffer]);
}

#[gpui::test]
async fn test_linked_action_log_buffer_edited(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 0)..Point::new(1, 3), "DEF")], None, cx)
                .unwrap();
        });
        child_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer,
        vec![HunkStatus {
            range: Point::new(1, 0)..Point::new(2, 0),
            diff_status: DiffHunkStatusKind::Modified,
            old_text: "def\n".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the agent edit"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the agent edit via linked log forwarding"
    );
}

#[gpui::test]
async fn test_linked_action_log_buffer_created(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("hello", cx));
        child_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer.clone(),
        vec![HunkStatus {
            range: Point::new(0, 0)..Point::new(0, 5),
            diff_status: DiffHunkStatusKind::Added,
            old_text: "".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the created file"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the created file via linked log forwarding"
    );
}

#[gpui::test]
async fn test_linked_action_log_will_delete_buffer(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello\n"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.will_delete_buffer(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.delete_file(file_path, false, cx))
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer.clone(),
        vec![HunkStatus {
            range: Point::new(0, 0)..Point::new(0, 0),
            diff_status: DiffHunkStatusKind::Deleted,
            old_text: "hello\n".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the deleted file"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the deleted file via linked log forwarding"
    );
}

/// Simulates the subagent scenario: two child logs linked to the same parent, each
/// editing a different file. The parent accumulates all edits while each child
/// only sees its own.
#[gpui::test]
async fn test_linked_action_log_independent_tracking(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a": "content of a",
            "file_b": "content of b",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log_1 =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));
    let child_log_2 =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_a_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/file_a", cx)
        })
        .unwrap();
    let file_b_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/file_b", cx)
        })
        .unwrap();
    let buffer_a = project
        .update(cx, |project, cx| project.open_buffer(file_a_path, cx))
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| project.open_buffer(file_b_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log_1.update(cx, |log, cx| log.buffer_read(buffer_a.clone(), cx));
        buffer_a.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "MODIFIED: ")], None, cx).unwrap();
        });
        child_log_1.update(cx, |log, cx| log.buffer_edited(buffer_a.clone(), cx));

        child_log_2.update(cx, |log, cx| log.buffer_read(buffer_b.clone(), cx));
        buffer_b.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "MODIFIED: ")], None, cx).unwrap();
        });
        child_log_2.update(cx, |log, cx| log.buffer_edited(buffer_b.clone(), cx));
    });
    cx.run_until_parked();

    let child_1_changed: Vec<_> = cx.read(|cx| {
        child_log_1
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });
    let child_2_changed: Vec<_> = cx.read(|cx| {
        child_log_2
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });
    let parent_changed: Vec<_> = cx.read(|cx| {
        parent_log
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });

    assert_eq!(
        child_1_changed,
        vec![buffer_a.clone()],
        "child 1 should only track file_a"
    );
    assert_eq!(
        child_2_changed,
        vec![buffer_b.clone()],
        "child 2 should only track file_b"
    );
    assert_eq!(parent_changed.len(), 2, "parent should track both files");
    assert!(
        parent_changed.contains(&buffer_a) && parent_changed.contains(&buffer_b),
        "parent should contain both buffer_a and buffer_b"
    );
}

#[gpui::test]
async fn test_file_read_time_recorded_on_buffer_read(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let abs_path = PathBuf::from(path!("/dir/file"));
    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "file_read_time should be None before buffer_read"
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
    });

    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_some()),
        "file_read_time should be recorded after buffer_read"
    );
}

#[gpui::test]
async fn test_file_read_time_recorded_on_buffer_edited(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let abs_path = PathBuf::from(path!("/dir/file"));
    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "file_read_time should be None before buffer_edited"
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });

    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_some()),
        "file_read_time should be recorded after buffer_edited"
    );
}

#[gpui::test]
async fn test_file_read_time_recorded_on_buffer_created(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "existing content"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let abs_path = PathBuf::from(path!("/dir/file"));
    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "file_read_time should be None before buffer_created"
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
    });

    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_some()),
        "file_read_time should be recorded after buffer_created"
    );
}

#[gpui::test]
async fn test_file_read_time_removed_on_delete(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let abs_path = PathBuf::from(path!("/dir/file"));

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
    });
    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_some()),
        "file_read_time should exist after buffer_read"
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.will_delete_buffer(buffer.clone(), cx));
    });
    assert!(
        action_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "file_read_time should be removed after will_delete_buffer"
    );
}

#[gpui::test]
async fn test_file_read_time_not_forwarded_to_linked_action_log(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let abs_path = PathBuf::from(path!("/dir/file"));

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
    });
    assert!(
        child_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_some()),
        "child should record file_read_time on buffer_read"
    );
    assert!(
        parent_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "parent should NOT get file_read_time from child's buffer_read"
    );

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    assert!(
        parent_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "parent should NOT get file_read_time from child's buffer_edited"
    );

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
    });
    assert!(
        parent_log.read_with(cx, |log, _| log.file_read_time(&abs_path).is_none()),
        "parent should NOT get file_read_time from child's buffer_created"
    );
}

#[derive(Debug, PartialEq)]
struct HunkStatus {
    range: Range<Point>,
    diff_status: DiffHunkStatusKind,
    old_text: String,
}

fn unreviewed_hunks(
    action_log: &Entity<ActionLog>,
    cx: &TestAppContext,
) -> Vec<(Entity<Buffer>, Vec<HunkStatus>)> {
    cx.read(|cx| {
        action_log
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, diff)| {
                let snapshot = buffer.read(cx).snapshot();
                (
                    buffer,
                    diff.read(cx)
                        .snapshot(cx)
                        .hunks(&snapshot)
                        .map(|hunk| HunkStatus {
                            diff_status: hunk.status().kind,
                            range: hunk.range,
                            old_text: diff
                                .read(cx)
                                .base_text(cx)
                                .text_for_range(hunk.diff_base_byte_range)
                                .collect(),
                        })
                        .collect(),
                )
            })
            .collect()
    })
}
