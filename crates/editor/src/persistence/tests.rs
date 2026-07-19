use super::*;

#[gpui::test]
async fn test_save_and_get_serialized_editor(cx: &mut gpui::TestAppContext) {
    let db = cx.update(|cx| workspace::WorkspaceDb::global(cx));
    let workspace_id = db.next_id().await.unwrap();
    let editor_db = cx.update(|cx| EditorDb::global(cx));

    let serialized_editor = SerializedEditor {
        abs_path: Some(PathBuf::from("testing.txt")),
        contents: None,
        language: None,
        mtime: None,
    };

    editor_db
        .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
        .await
        .unwrap();

    let have = editor_db
        .get_serialized_editor(1234, workspace_id)
        .unwrap()
        .unwrap();
    assert_eq!(have, serialized_editor);

    let serialized_editor = SerializedEditor {
        abs_path: Some(PathBuf::from("testing.txt")),
        contents: Some("Test".to_owned()),
        language: Some("Go".to_owned()),
        mtime: None,
    };

    editor_db
        .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
        .await
        .unwrap();

    let have = editor_db
        .get_serialized_editor(1234, workspace_id)
        .unwrap()
        .unwrap();
    assert_eq!(have, serialized_editor);

    let serialized_editor = SerializedEditor {
        abs_path: None,
        contents: None,
        language: None,
        mtime: None,
    };

    editor_db
        .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
        .await
        .unwrap();

    let have = editor_db
        .get_serialized_editor(1234, workspace_id)
        .unwrap()
        .unwrap();
    assert_eq!(have, serialized_editor);

    let serialized_editor = SerializedEditor {
        abs_path: None,
        contents: None,
        language: None,
        mtime: Some(MTime::from_seconds_and_nanos(100, 42)),
    };

    editor_db
        .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
        .await
        .unwrap();

    let have = editor_db
        .get_serialized_editor(1234, workspace_id)
        .unwrap()
        .unwrap();
    assert_eq!(have, serialized_editor);
}

#[gpui::test]
async fn test_save_and_get_file_folds(cx: &mut gpui::TestAppContext) {
    let db = cx.update(|cx| workspace::WorkspaceDb::global(cx));
    let workspace_id = db.next_id().await.unwrap();
    let editor_db = cx.update(|cx| EditorDb::global(cx));

    let file_path: Arc<Path> = Arc::from(Path::new("/tmp/test_file_folds.rs"));

    let folds = vec![
        (
            100,
            200,
            "fn main() {".to_string(),
            "} // end main".to_string(),
        ),
        (
            300,
            400,
            "struct Foo {".to_string(),
            "} // end Foo".to_string(),
        ),
    ];
    editor_db
        .save_file_folds(workspace_id, file_path.clone(), folds.clone())
        .await
        .unwrap();

    let retrieved = editor_db.get_file_folds(workspace_id, &file_path).unwrap();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(
        retrieved[0],
        (
            100,
            200,
            Some("fn main() {".to_string()),
            Some("} // end main".to_string())
        )
    );
    assert_eq!(
        retrieved[1],
        (
            300,
            400,
            Some("struct Foo {".to_string()),
            Some("} // end Foo".to_string())
        )
    );

    let new_folds = vec![(
        500,
        600,
        "impl Bar {".to_string(),
        "} // end impl".to_string(),
    )];
    editor_db
        .save_file_folds(workspace_id, file_path.clone(), new_folds)
        .await
        .unwrap();

    let retrieved = editor_db.get_file_folds(workspace_id, &file_path).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(
        retrieved[0],
        (
            500,
            600,
            Some("impl Bar {".to_string()),
            Some("} // end impl".to_string())
        )
    );

    editor_db
        .delete_file_folds(workspace_id, file_path.clone())
        .await
        .unwrap();
    let retrieved = editor_db.get_file_folds(workspace_id, &file_path).unwrap();
    assert!(retrieved.is_empty());

    let file_path_a: Arc<Path> = Arc::from(Path::new("/tmp/file_a.rs"));
    let file_path_b: Arc<Path> = Arc::from(Path::new("/tmp/file_b.rs"));
    let folds_a = vec![(10, 20, "a_start".to_string(), "a_end".to_string())];
    let folds_b = vec![(30, 40, "b_start".to_string(), "b_end".to_string())];

    editor_db
        .save_file_folds(workspace_id, file_path_a.clone(), folds_a)
        .await
        .unwrap();
    editor_db
        .save_file_folds(workspace_id, file_path_b.clone(), folds_b)
        .await
        .unwrap();

    let retrieved_a = editor_db
        .get_file_folds(workspace_id, &file_path_a)
        .unwrap();
    let retrieved_b = editor_db
        .get_file_folds(workspace_id, &file_path_b)
        .unwrap();

    assert_eq!(retrieved_a.len(), 1);
    assert_eq!(retrieved_b.len(), 1);
    assert_eq!(retrieved_a[0].0, 10);
    assert_eq!(retrieved_b[0].0, 30);
}
