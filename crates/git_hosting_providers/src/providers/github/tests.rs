use git::repository::repo_path;
use indoc::indoc;
use pretty_assertions::assert_eq;

use super::*;

#[test]
fn test_remote_url_with_root_slash() {
    let remote_url = "git@github.com:/mav-industries/mav";
    let parsed_remote = Github::public_instance()
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
fn test_invalid_self_hosted_remote_url() {
    let remote_url = "git@github.com:mav-industries/mav.git";
    let github = Github::from_remote_url(remote_url);
    assert!(github.is_err());
}

#[test]
fn test_from_remote_url_ssh() {
    let remote_url = "git@github.my-enterprise.com:mav-industries/mav.git";
    let github = Github::from_remote_url(remote_url).unwrap();

    assert!(!github.supports_avatars());
    assert_eq!(github.name, "GitHub Self-Hosted".to_string());
    assert_eq!(
        github.base_url,
        Url::parse("https://github.my-enterprise.com").unwrap()
    );
}

#[test]
fn test_from_remote_url_https() {
    let remote_url = "https://github.my-enterprise.com/mav-industries/mav.git";
    let github = Github::from_remote_url(remote_url).unwrap();

    assert!(!github.supports_avatars());
    assert_eq!(github.name, "GitHub Self-Hosted".to_string());
    assert_eq!(
        github.base_url,
        Url::parse("https://github.my-enterprise.com").unwrap()
    );
}

#[test]
fn test_parse_remote_url_given_self_hosted_ssh_url() {
    let remote_url = "git@github.my-enterprise.com:mav-industries/mav.git";
    let parsed_remote = Github::from_remote_url(remote_url)
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
fn test_parse_remote_url_given_self_hosted_https_url_with_subgroup() {
    let remote_url = "https://github.my-enterprise.com/mav-industries/mav.git";
    let parsed_remote = Github::from_remote_url(remote_url)
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
fn test_parse_remote_url_given_ssh_url() {
    let parsed_remote = Github::public_instance()
        .parse_remote_url("git@github.com:mav-industries/mav.git")
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
    let parsed_remote = Github::public_instance()
        .parse_remote_url("https://github.com/mav-industries/mav.git")
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
    let parsed_remote = Github::public_instance()
        .parse_remote_url("https://jlannister@github.com/some-org/some-repo.git")
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "some-org".into(),
            repo: "some-repo".into(),
        }
    );
}

#[test]
fn test_build_github_permalink_from_ssh_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };
    let permalink = Github::public_instance().build_permalink(
        remote,
        BuildPermalinkParams::new(
            "e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7",
            &repo_path("crates/editor/src/git/permalink.rs"),
            None,
        ),
    );

    let expected_url = "https://github.com/mav-industries/mav/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_github_permalink() {
    let permalink = Github::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new(
            "b2efec9824c45fcc90c9a7eb107a50d1772a60aa",
            &repo_path("crates/mav/src/main.rs"),
            None,
        ),
    );

    let expected_url = "https://github.com/mav-industries/mav/blob/b2efec9824c45fcc90c9a7eb107a50d1772a60aa/crates/mav/src/main.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_github_permalink_with_single_line_selection() {
    let permalink = Github::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new(
            "e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7",
            &repo_path("crates/editor/src/git/permalink.rs"),
            Some(6..6),
        ),
    );

    let expected_url = "https://github.com/mav-industries/mav/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs#L7";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_github_permalink_with_multi_line_selection() {
    let permalink = Github::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new(
            "e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7",
            &repo_path("crates/editor/src/git/permalink.rs"),
            Some(23..47),
        ),
    );

    let expected_url = "https://github.com/mav-industries/mav/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs#L24-L48";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_github_create_pr_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let provider = Github::public_instance();

    let url = provider
        .build_create_pull_request_url(&remote, "feature/something cool")
        .expect("url should be constructed");

    assert_eq!(
        url.as_str(),
        "https://github.com/mav-industries/mav/pull/new/feature%2Fsomething%20cool"
    );
}

#[test]
fn test_github_pull_requests() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let github = Github::public_instance();
    let message = "This does not contain a pull request";
    assert!(github.extract_pull_request(&remote, message).is_none());

    let message = indoc! {r#"
        project panel: do not expand collapsed worktrees on "collapse all entries" (#10687)

        Fixes #10597

        Release Notes:

        - Fixed "project panel: collapse all entries" expanding collapsed worktrees.
        "#
    };

    assert_eq!(
        github
            .extract_pull_request(&remote, message)
            .unwrap()
            .url
            .as_str(),
        "https://github.com/mav-industries/mav/pull/10687"
    );

    let message = indoc! {r#"
        Follow-up to #10687 to fix problems

        See the original PR, this is a fix.
        "#
    };
    assert_eq!(github.extract_pull_request(&remote, message), None);
}

/// Regression test for issue #39875
#[test]
fn test_git_permalink_url_escaping() {
    let permalink = Github::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "nonexistent".into(),
        },
        BuildPermalinkParams::new(
            "3ef1539900037dd3601be7149b2b39ed6d0ce3db",
            &repo_path("app/blog/[slug]/page.tsx"),
            Some(7..7),
        ),
    );

    let expected_url = "https://github.com/mav-industries/nonexistent/blob/3ef1539900037dd3601be7149b2b39ed6d0ce3db/app/blog/%5Bslug%5D/page.tsx#L8";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_create_pull_request_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let github = Github::public_instance();
    let url = github
        .build_create_pull_request_url(&remote, "feature/new-feature")
        .unwrap();

    assert_eq!(
        url.as_str(),
        "https://github.com/mav-industries/mav/pull/new/feature%2Fnew-feature"
    );

    let base_url = Url::parse("https://github.mav.com").unwrap();
    let github = Github::new("GitHub Self-Hosted", base_url);
    let url = github
        .build_create_pull_request_url(&remote, "feature/new-feature")
        .expect("should be able to build pull request url");

    assert_eq!(
        url.as_str(),
        "https://github.mav.com/mav-industries/mav/pull/new/feature%2Fnew-feature"
    );
}

#[test]
fn test_build_cdn_avatar_url_simple_email() {
    let url = build_cdn_avatar_url("user@example.com").unwrap();
    assert_eq!(
        url.as_str(),
        "https://avatars.githubusercontent.com/u/e?email=user%40example.com&s=128"
    );
}

#[test]
fn test_build_cdn_avatar_url_with_angle_brackets() {
    let url = build_cdn_avatar_url("<user@example.com>").unwrap();
    assert_eq!(
        url.as_str(),
        "https://avatars.githubusercontent.com/u/e?email=user%40example.com&s=128"
    );
}

#[test]
fn test_build_cdn_avatar_url_with_special_chars() {
    let url = build_cdn_avatar_url("user+tag@example.com").unwrap();
    assert_eq!(
        url.as_str(),
        "https://avatars.githubusercontent.com/u/e?email=user%2Btag%40example.com&s=128"
    );
}

#[test]
fn test_build_cdn_avatar_url_for_author_email_skips_bot_noreply_emails() {
    for email in [
        "41898282+github-actions[bot]@users.noreply.github.com",
        "<41898282+github-actions[bot]@users.noreply.github.com>",
    ] {
        assert_eq!(build_cdn_avatar_url_for_author_email(email).unwrap(), None);
    }
}

#[test]
fn test_build_cdn_avatar_url_for_author_email_uses_user_noreply_emails() {
    let url = build_cdn_avatar_url_for_author_email("12345+octocat@users.noreply.github.com")
        .unwrap()
        .unwrap();

    assert_eq!(
        url.as_str(),
        "https://avatars.githubusercontent.com/u/e?email=12345%2Boctocat%40users.noreply.github.com&s=128"
    );
}
