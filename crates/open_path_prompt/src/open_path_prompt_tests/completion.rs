use super::*;

#[gpui::test]
async fn test_open_path_prompt_completion(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": "A",
                "dir1": {},
                "dir2": {
                    "c": "C",
                    "d": "D",
                    "dir3": {},
                    "dir4": {}
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, cx) = build_open_path_prompt(project, false, false, cx);

    // Confirm completion for the query "/root", since it's a directory, it should add a trailing slash.
    let query = path!("/root");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 0, &picker, cx).unwrap(),
        path!("/root/")
    );

    // Confirm completion for the query "/root/", selecting the first candidate "dir1", since it's a directory, it should add a trailing slash.
    let query = path!("/root/");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 0, &picker, cx),
        None,
        "First entry is `./` and when we confirm completion, it is tabbed below"
    );
    assert_eq!(
        confirm_completion(query, 1, &picker, cx).unwrap(),
        path!("/root/dir1/"),
        "Second entry is the first directory"
    );

    // Confirm completion for the query "/root/", selecting the third candidate "a", since it's a file, it should not add a trailing slash.
    let query = path!("/root/");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 3, &picker, cx).unwrap(),
        path!("/root/a")
    );

    let query = path!("/root/a");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 0, &picker, cx).unwrap(),
        path!("/root/a")
    );

    let query = path!("/root/d");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 1, &picker, cx).unwrap(),
        path!("/root/dir2/")
    );

    let query = path!("/root/dir2");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 0, &picker, cx).unwrap(),
        path!("/root/dir2/")
    );

    let query = path!("/root/dir2/");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 1, &picker, cx).unwrap(),
        path!("/root/dir2/dir3/")
    );

    let query = path!("/root/dir2/");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 3, &picker, cx).unwrap(),
        path!("/root/dir2/c")
    );

    let query = path!("/root/dir2/d");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 0, &picker, cx).unwrap(),
        path!("/root/dir2/dir3/")
    );

    let query = path!("/root/dir2/d");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 1, &picker, cx).unwrap(),
        path!("/root/dir2/dir4/")
    );

    let query = path!("/root/dir2/di");
    insert_query(query, &picker, cx).await;
    assert_eq!(
        confirm_completion(query, 1, &picker, cx).unwrap(),
        path!("/root/dir2/dir4/")
    );
}
