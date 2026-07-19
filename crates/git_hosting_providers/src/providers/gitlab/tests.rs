use git::repository::repo_path;
use pretty_assertions::assert_eq;

use super::*;

#[test]
fn test_invalid_self_hosted_remote_url() {
    let remote_url = "https://gitlab.com/mav-industries/mav.git";
    let gitlab = Gitlab::from_remote_url(remote_url);
    assert!(gitlab.is_err());
}

#[test]
fn test_parse_remote_url_given_ssh_url() {
    let parsed_remote = Gitlab::public_instance()
        .parse_remote_url("git@gitlab.com:mav-industries/mav.git")
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
    let parsed_remote = Gitlab::public_instance()
        .parse_remote_url("https://gitlab.com/mav-industries/mav.git")
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
    let remote_url = "git@gitlab.my-enterprise.com:mav-industries/mav.git";

    let parsed_remote = Gitlab::from_remote_url(remote_url)
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
    let remote_url = "https://gitlab.my-enterprise.com/group/subgroup/mav.git";
    let parsed_remote = Gitlab::from_remote_url(remote_url)
        .unwrap()
        .parse_remote_url(remote_url)
        .unwrap();

    assert_eq!(
        parsed_remote,
        ParsedGitRemote {
            owner: "group/subgroup".into(),
            repo: "mav".into(),
        }
    );
}

#[test]
fn test_build_gitlab_permalink() {
    let permalink = Gitlab::public_instance().build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new(
            "e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7",
            &repo_path("crates/editor/src/git/permalink.rs"),
            None,
        ),
    );

    let expected_url = "https://gitlab.com/mav-industries/mav/-/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_gitlab_permalink_with_single_line_selection() {
    let permalink = Gitlab::public_instance().build_permalink(
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

    let expected_url = "https://gitlab.com/mav-industries/mav/-/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs#L7";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_gitlab_permalink_with_multi_line_selection() {
    let permalink = Gitlab::public_instance().build_permalink(
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

    let expected_url = "https://gitlab.com/mav-industries/mav/-/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs#L24-48";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_gitlab_create_pr_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let provider = Gitlab::public_instance();

    let url = provider
        .build_create_pull_request_url(&remote, "feature/cool stuff")
        .expect("create PR url should be constructed");

    assert_eq!(
        url.as_str(),
        "https://gitlab.com/mav-industries/mav/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fcool%20stuff"
    );
}

#[test]
fn test_build_gitlab_self_hosted_permalink_from_ssh_url() {
    let gitlab =
        Gitlab::from_remote_url("git@gitlab.some-enterprise.com:mav-industries/mav.git").unwrap();
    let permalink = gitlab.build_permalink(
        ParsedGitRemote {
            owner: "mav-industries".into(),
            repo: "mav".into(),
        },
        BuildPermalinkParams::new(
            "e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7",
            &repo_path("crates/editor/src/git/permalink.rs"),
            None,
        ),
    );

    let expected_url = "https://gitlab.some-enterprise.com/mav-industries/mav/-/blob/e6ebe7974deb6bb6cc0e2595c8ec31f0c71084b7/crates/editor/src/git/permalink.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_gitlab_self_hosted_permalink_from_https_url() {
    let gitlab =
        Gitlab::from_remote_url("https://gitlab-instance.big-co.com/mav-industries/mav.git")
            .unwrap();
    let permalink = gitlab.build_permalink(
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

    let expected_url = "https://gitlab-instance.big-co.com/mav-industries/mav/-/blob/b2efec9824c45fcc90c9a7eb107a50d1772a60aa/crates/mav/src/main.rs";
    assert_eq!(permalink.to_string(), expected_url.to_string())
}

#[test]
fn test_build_create_pull_request_url() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let github = Gitlab::public_instance();
    let url = github
        .build_create_pull_request_url(&remote, "feature/new-feature")
        .unwrap();

    assert_eq!(
        url.as_str(),
        "https://gitlab.com/mav-industries/mav/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fnew-feature"
    );

    let base_url = Url::parse("https://gitlab.mav.com").unwrap();
    let github = Gitlab::new("GitLab Self-Hosted", base_url);
    let url = github
        .build_create_pull_request_url(&remote, "feature/new-feature")
        .expect("should be able to build pull request url");

    assert_eq!(
        url.as_str(),
        "https://gitlab.mav.com/mav-industries/mav/-/merge_requests/new?merge_request%5Bsource_branch%5D=feature%2Fnew-feature"
    );
}

#[test]
fn test_extract_merge_request_from_squash_commit() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let provider = Gitlab::public_instance();

    // Test squash merge pattern: "commit message (!123)"
    let message = "Add new feature (!456)";
    let pull_request = provider.extract_pull_request(&remote, message).unwrap();

    assert_eq!(pull_request.number, 456);
    assert_eq!(
        pull_request.url.as_str(),
        "https://gitlab.com/mav-industries/mav/-/merge_requests/456"
    );
}

#[test]
fn test_extract_merge_request_from_merge_commit() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let provider = Gitlab::public_instance();

    // Test standard merge commit pattern: "See merge request group/project!123"
    let message = "Merge branch 'feature' into 'main'\n\nSee merge request mav-industries/mav!789";
    let pull_request = provider.extract_pull_request(&remote, message).unwrap();

    assert_eq!(pull_request.number, 789);
    assert_eq!(
        pull_request.url.as_str(),
        "https://gitlab.com/mav-industries/mav/-/merge_requests/789"
    );
}

#[test]
fn test_extract_merge_request_self_hosted() {
    let base_url = Url::parse("https://gitlab.my-company.com").unwrap();
    let provider = Gitlab::new("GitLab Self-Hosted", base_url);

    let remote = ParsedGitRemote {
        owner: "team".into(),
        repo: "project".into(),
    };

    let message = "Fix bug (!42)";
    let pull_request = provider.extract_pull_request(&remote, message).unwrap();

    assert_eq!(pull_request.number, 42);
    assert_eq!(
        pull_request.url.as_str(),
        "https://gitlab.my-company.com/team/project/-/merge_requests/42"
    );
}

#[test]
fn test_extract_merge_request_no_match() {
    let remote = ParsedGitRemote {
        owner: "mav-industries".into(),
        repo: "mav".into(),
    };

    let provider = Gitlab::public_instance();

    // No MR reference in message
    let message = "Just a regular commit message";
    let pull_request = provider.extract_pull_request(&remote, message);

    assert!(pull_request.is_none());
}
