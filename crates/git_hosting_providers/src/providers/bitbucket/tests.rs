use super::*;

#[test]
fn test_parse_remote_url_given_ssh_url() {
    let parsed_remote = Bitbucket::public_instance()
        .parse_remote_url("git@bitbucket.org:mav-industries/mav.git")
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_parse_remote_url_given_https_url() {
    let parsed_remote = Bitbucket::public_instance()
        .parse_remote_url("https://bitbucket.org/mav-industries/mav.git")
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_parse_remote_url_given_https_url_with_username() {
    let parsed_remote = Bitbucket::public_instance()
        .parse_remote_url("https://thorstenballmav@bitbucket.org/mav-industries/mav.git")
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_parse_remote_url_given_self_hosted_ssh_url() {
    let remote_url = "git@bitbucket.company.com:mav-industries/mav.git";

    let parsed_remote = Bitbucket::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_parse_remote_url_given_self_hosted_https_url() {
    let remote_url = "https://bitbucket.company.com/mav-industries/mav.git";

    let parsed_remote = Bitbucket::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );

    // Test with "scm" in the path
    let remote_url = "https://bitbucket.company.com/scm/mav-industries/mav.git";

    let parsed_remote = Bitbucket::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );

    // Test with only "scm" as owner
    let remote_url = "https://bitbucket.company.com/scm/mav.git";

    let parsed_remote = Bitbucket::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "scm".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_parse_remote_url_given_self_hosted_https_url_with_username() {
    let remote_url = "https://thorstenballmav@bitbucket.company.com/mav-industries/mav.git";

    let parsed_remote = Bitbucket::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_build_bitbucket_permalink() {
    let permalink = Bitbucket::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), None),
    );

    let expected_url = "https://bitbucket.org/mav-industries/mav/src/f00b4r/main.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_self_hosted_permalink() {
    let permalink = Bitbucket::from_remote_url("git@bitbucket.company.com:mav-industries/mav.git")
        .unwrap()
        .build_permalink(
            ParsedGitRemote {
                owner: "mav-industries".into(),
                repo: "mav".into(),
            },
            BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), None),
        );

    let expected_url =
        "https://bitbucket.company.com/projects/mav-industries/repos/mav/browse/main.rs?at=f00b4r";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_permalink_with_single_line_selection() {
    let permalink = Bitbucket::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), Some(6..6)),
    );

    let expected_url = "https://bitbucket.org/mav-industries/mav/src/f00b4r/main.rs#lines-7";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_self_hosted_permalink_with_single_line_selection() {
    let permalink =
        Bitbucket::from_remote_url("https://bitbucket.company.com/mav-industries/mav.git")
            .unwrap()
            .build_permalink(
                ParsedGitRemote {
                    owner: "mav-industries".into(),
                    repo: "mav".into(),
                },
                BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), Some(6..6)),
            );

    let expected_url = "https://bitbucket.company.com/projects/mav-industries/repos/mav/browse/main.rs?at=f00b4r#7";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_permalink_with_multi_line_selection() {
    let permalink = Bitbucket::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), Some(23..47)),
    );

    let expected_url = "https://bitbucket.org/mav-industries/mav/src/f00b4r/main.rs#lines-24:48";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_self_hosted_permalink_with_multi_line_selection() {
    let permalink = Bitbucket::from_remote_url("git@bitbucket.company.com:mav-industries/mav.git")
        .unwrap()
        .build_permalink(
            ParsedGitRemote {
                owner: "mav-industries".into(),
                repo: "mav".into(),
            },
            BuildPermalinkParams::new("f00b4r", &repo_path("main.rs"), Some(23..47)),
        );

    let expected_url = "https://bitbucket.company.com/projects/mav-industries/repos/mav/browse/main.rs?at=f00b4r#24-48";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_bitbucket_create_pr_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let url = Bitbucket::public_instance()
        .build_create_pull_request_url(&remote, "feature/my-branch")
        .expect("url should be constructed");

    assert_eq!(
        url.as_str(),
        "https://bitbucket.org/mav-industries/mav/pull-requests/new?source=feature%2Fmy-branch"
    );
}

#[test]
fn test_build_bitbucket_self_hosted_create_pr_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let url = Bitbucket::from_remote_url("https://bitbucket.company.com/mav-industries/mav.git")
        .unwrap()
        .build_create_pull_request_url(&remote, "feature/my-branch")
        .expect("url should be constructed");

    assert_eq!(
        url.as_str(),
        "https://bitbucket.company.com/projects/mav-industries/repos/mav/compare/commits?sourceBranch=refs%2Fheads%2Ffeature%2Fmy-branch"
    );
}

#[test]
fn test_bitbucket_pull_requests() {
    use indoc::indoc;

    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let bitbucket = Bitbucket::public_instance();

    // Test message without PR reference
    let message = "This does not contain a pull request";
    assert!(bitbucket.extract_pull_request(&remote, message).is_none());

    // Pull request number at end of first line
    let message = indoc! {r#"
            Merged in feature-branch (pull request #123)

            Some detailed description of the changes.
        "#};

    let pr = bitbucket.extract_pull_request(&remote, message).unwrap();
    assert_eq!(pr.number, 123);
    assert_eq!(
        pr.url.as_str(),
        "https://bitbucket.org/mav-industries/mav/pull-requests/123"
    );
}

#[test]
fn test_bitbucket_self_hosted_pull_requests() {
    use indoc::indoc;

    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let bitbucket =
        Bitbucket::from_remote_url("https://bitbucket.company.com/mav-industries/mav.git").unwrap();

    // Test message without PR reference
    let message = "This does not contain a pull request";
    assert!(bitbucket.extract_pull_request(&remote, message).is_none());

    // Pull request number at end of first line
    let message = indoc! {r#"
            Merged in feature-branch (pull request #123)

            Some detailed description of the changes.
        "#};

    let pr = bitbucket.extract_pull_request(&remote, message).unwrap();
    assert_eq!(pr.number, 123);
    assert_eq!(
        pr.url.as_str(),
        "https://bitbucket.company.com/projects/mav-industries/repos/mav/pull-requests/123"
    );
}
