use super::common::*;

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
