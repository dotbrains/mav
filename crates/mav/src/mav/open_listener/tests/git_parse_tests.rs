use super::*;

#[gpui::test]
fn test_parse_git_commit_url(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    // Test basic git commit URL
    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://git/commit/abc123?repo=path/to/repo".into()],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind.unwrap() {
        OpenRequestKind::GitCommit { sha } => {
            assert_eq!(sha, "abc123");
        }
        _ => panic!("expected GitCommit variant"),
    }
    // Verify path was added to open_paths for workspace routing
    assert_eq!(request.open_paths, vec!["path/to/repo"]);

    // Test with URL encoded path
    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://git/commit/def456?repo=path%20with%20spaces".into()],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind.unwrap() {
        OpenRequestKind::GitCommit { sha } => {
            assert_eq!(sha, "def456");
        }
        _ => panic!("expected GitCommit variant"),
    }
    assert_eq!(request.open_paths, vec!["path with spaces"]);

    // Test with empty path
    cx.update(|cx| {
        assert!(
            OpenRequest::parse(
                RawOpenRequest {
                    urls: vec!["mav://git/commit/abc123?repo=".into()],
                    ..Default::default()
                },
                cx,
            )
            .unwrap_err()
            .to_string()
            .contains("missing repo")
        );
    });

    // Test error case: missing SHA
    let result = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://git/commit/abc123?foo=bar".into()],
                ..Default::default()
            },
            cx,
        )
    });
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing repo query parameter")
    );
}

#[gpui::test]
fn test_parse_git_clone_url(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec![
                    "mav://git/clone/?repo=https://github.com/mav-industries/mav.git".into(),
                ],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::GitClone { repo_url }) => {
            assert_eq!(repo_url, "https://github.com/mav-industries/mav.git");
        }
        _ => panic!("Expected GitClone kind"),
    }
}

#[gpui::test]
fn test_parse_git_clone_url_without_slash(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://git/clone?repo=https://github.com/mav-industries/mav.git".into()],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::GitClone { repo_url }) => {
            assert_eq!(repo_url, "https://github.com/mav-industries/mav.git");
        }
        _ => panic!("Expected GitClone kind"),
    }
}

#[gpui::test]
fn test_parse_git_clone_url_with_encoding(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec![
                    "mav://git/clone/?repo=https%3A%2F%2Fgithub.com%2Fmav-industries%2Fmav.git"
                        .into(),
                ],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::GitClone { repo_url }) => {
            assert_eq!(repo_url, "https://github.com/mav-industries/mav.git");
        }
        _ => panic!("Expected GitClone kind"),
    }
}
