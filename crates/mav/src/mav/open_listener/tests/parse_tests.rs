use super::*;

fn assert_ssh_parse(
    cx: &mut TestAppContext,
    input: &str,
    expected_url: Option<&str>,
    host: &str,
    username: Option<&str>,
    port: Option<u16>,
    path: &str,
) {
    if let Some(expected_url) = expected_url {
        assert_eq!(parse_ssh_url(input).unwrap().as_str(), expected_url);
    }

    let request = cx.update(|cx| {
        let rq = RawOpenRequest {
            urls: vec![input.into()],
            ..Default::default()
        };
        OpenRequest::parse(rq, cx).unwrap()
    });
    assert_eq!(
        request.remote_connection.unwrap(),
        RemoteConnectionOptions::Ssh(SshConnectionOptions {
            host: host.into(),
            username: username.map(str::to_string),
            port,
            ..Default::default()
        })
    );
    assert_eq!(request.open_paths, vec![path]);
}

#[gpui::test]
fn test_parse_ssh_urls(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);
    let cases = [
        ("ssh://me@host:/", None, "host", Some("me"), None, "/"),
        (
            "ssh://me@host:~/code",
            None,
            "host",
            Some("me"),
            None,
            "/~/code",
        ),
        (
            "ssh://me@host:22/tmp",
            None,
            "host",
            Some("me"),
            Some(22),
            "/tmp",
        ),
        (
            "ssh://user@domain.tld@host:22/tmp",
            None,
            "host",
            Some("user@domain.tld"),
            Some(22),
            "/tmp",
        ),
        (
            "ssh://domain\\user@host/dir",
            Some("ssh://domain%5Cuser@host/dir"),
            "host",
            Some("domain\\user"),
            None,
            "/dir",
        ),
        (
            r"ssh://domain\\user@localhost/project",
            Some("ssh://domain%5C%5Cuser@localhost/project"),
            "localhost",
            Some(r"domain\\user"),
            None,
            "/project",
        ),
    ];

    for (input, expected_url, host, username, port, path) in cases {
        assert_ssh_parse(cx, input, expected_url, host, username, port, path);
    }
}

#[gpui::test]
async fn test_derive_paths_with_position_directory_with_position_like_name(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    let fs = app_state.fs.as_fake();

    // A folder whose name ends in `(N)` or `(row,col)` would otherwise be parsed as a
    // path with a row/column suffix (e.g. the MSVC-style `file.c(22)`), truncating the name.
    fs.insert_tree(
        path!("/root"),
        json!({
            "TEST (1)": {},
            "Project (2,3)": {},
            "test 123": {},
        }),
    )
    .await;

    let inputs = vec![
        path!("/root/TEST (1)").to_string(),
        path!("/root/Project (2,3)").to_string(),
        path!("/root/test 123").to_string(),
    ];
    let result = derive_paths_with_position(fs.as_ref(), inputs).await;

    let paths: Vec<_> = result
        .iter()
        .map(|p| (p.path.to_string_lossy().to_string(), p.row, p.column))
        .collect();
    assert_eq!(
        paths,
        vec![
            (path!("/root/TEST (1)").to_string(), None, None),
            (path!("/root/Project (2,3)").to_string(), None, None),
            (path!("/root/test 123").to_string(), None, None),
        ]
    );
}

// Test file with colon (`:`) in the name on non-Windows platforms,
// as it is valid for file names on Unix-like systems.
#[cfg(not(target_os = "windows"))]
#[gpui::test]
async fn test_derive_paths_with_position_colon_in_name_reverts_on_unix(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    let fs = app_state.fs.as_fake();

    fs.insert_tree(path!("/root"), json!({ "test.txt:10": "" }))
        .await;

    let result =
        derive_paths_with_position(fs.as_ref(), vec![path!("/root/test.txt:10").to_string()]).await;

    let paths: Vec<_> = result
        .iter()
        .map(|p| (p.path.to_string_lossy().to_string(), p.row, p.column))
        .collect();
    assert_eq!(
        paths,
        vec![(path!("/root/test.txt:10").to_string(), None, None)]
    );
}

// On Windows `:` is used to delimit NTFS alternate data streams,
// `notes.txt:10` should be parsed as `notes.txt` at row 10
#[cfg(target_os = "windows")]
#[gpui::test]
async fn test_derive_paths_with_position_colon_in_name_parsed_as_position_on_windows(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    let fs = app_state.fs.as_fake();

    fs.insert_tree(path!("/root"), json!({ "test.txt": "" }))
        .await;

    let result =
        derive_paths_with_position(fs.as_ref(), vec![path!("/root/test.txt:10").to_string()]).await;

    let paths: Vec<_> = result
        .iter()
        .map(|p| (p.path.to_string_lossy().to_string(), p.row, p.column))
        .collect();
    assert_eq!(
        paths,
        vec![(path!("/root/test.txt").to_string(), Some(10), None)]
    );
}

#[gpui::test]
fn test_parse_ssh_url_preserves_open_behavior(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["ssh://me@host:/".into()],
                open_behavior: Some(cli::OpenBehavior::AlwaysNew),
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    assert_eq!(request.open_behavior, Some(cli::OpenBehavior::AlwaysNew));
}

#[gpui::test]
fn test_reject_ssh_urls(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    for input in [
        "ssh://me@localhost:code/vibes/mine-bot",
        "ssh://me@localhost:2222:~/project",
        "ssh://me@[2001:db8::1]:~/project",
    ] {
        let result = cx.update(|cx| {
            OpenRequest::parse(
                RawOpenRequest {
                    urls: vec![input.into()],
                    ..Default::default()
                },
                cx,
            )
        });
        assert!(result.is_err(), "{input} should be rejected");
    }
}

#[gpui::test]
fn test_open_options_for_behavior_always_new(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);
    let options = cx.update(|cx| {
        open_options_for_behavior(
            cli::OpenBehavior::AlwaysNew,
            &SerializedWorkspaceLocation::Local,
            cx,
        )
    });
    assert_eq!(
        options.workspace_matching,
        workspace::WorkspaceMatching::None
    );
    assert!(!options.add_dirs_to_sidebar);
    assert!(options.requesting_window.is_none());
}

#[gpui::test]
fn test_parse_agent_url(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://agent".into()],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::AgentPanel {
            external_source_prompt,
        }) => {
            assert_eq!(external_source_prompt, None);
        }
        _ => panic!("Expected AgentPanel kind"),
    }
}

#[gpui::test]
fn test_parse_skill_install_url(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let content =
        "---\nname: my-skill\ndescription: Does a thing.\n---\n\nDo the thing.\n".to_string();
    let link = agent_skills::encode_skill_share_link(&content);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec![link],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::InstallSkill {
            content: parsed_content,
        }) => {
            assert_eq!(parsed_content, content);
        }
        _ => panic!("Expected InstallSkill kind"),
    }
}

#[gpui::test]
fn test_parse_malformed_skill_install_url_errors(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let result = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://skill?data=!!!notbase64".into()],
                ..Default::default()
            },
            cx,
        )
    });

    assert!(result.is_err());
}

fn agent_url_with_prompt(prompt: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new("mav://agent?".to_string());
    serializer.append_pair("prompt", prompt);
    serializer.finish()
}

#[gpui::test]
fn test_parse_agent_url_with_prompt(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);
    let prompt = "Write me a script\nThanks";

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec![agent_url_with_prompt(prompt)],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::AgentPanel {
            external_source_prompt,
        }) => {
            assert_eq!(
                external_source_prompt
                    .as_ref()
                    .map(ExternalSourcePrompt::as_str),
                Some("Write me a script\nThanks")
            );
        }
        _ => panic!("Expected AgentPanel kind"),
    }
}

#[gpui::test]
fn test_parse_agent_url_with_trailing_slash(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec!["mav://agent/?prompt=hello".into()],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::AgentPanel {
            external_source_prompt,
        }) => {
            assert_eq!(
                external_source_prompt
                    .as_ref()
                    .map(ExternalSourcePrompt::as_str),
                Some("hello")
            );
        }
        _ => panic!("Expected AgentPanel kind"),
    }
}

#[gpui::test]
fn test_parse_focus_app_url(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    for url in ["mav://", "mav://open", "mav://open/"] {
        let request = cx.update(|cx| {
            OpenRequest::parse(
                RawOpenRequest {
                    urls: vec![url.into()],
                    ..Default::default()
                },
                cx,
            )
            .unwrap()
        });
        assert!(
            matches!(request.kind, Some(OpenRequestKind::FocusApp)),
            "expected FocusApp for {url}, got {:?}",
            request.kind
        );
        assert!(
            request.is_focus_app_only(),
            "expected is_focus_app_only for {url}"
        );
    }
}

#[gpui::test]
fn test_parse_agent_url_with_empty_prompt(cx: &mut TestAppContext) {
    let _app_state = init_test(cx);

    let request = cx.update(|cx| {
        OpenRequest::parse(
            RawOpenRequest {
                urls: vec![agent_url_with_prompt("")],
                ..Default::default()
            },
            cx,
        )
        .unwrap()
    });

    match request.kind {
        Some(OpenRequestKind::AgentPanel {
            external_source_prompt,
        }) => {
            assert_eq!(external_source_prompt, None);
        }
        _ => panic!("Expected AgentPanel kind"),
    }
}

#[path = "git_parse_tests.rs"]
mod git_parse_tests;
