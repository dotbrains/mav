use super::*;
use crate::remote_connections::{Connection, SshConnection};
use crate::remote_servers::{ProjectEntry, ServerIndex, SshServerIndex};
use settings::RemoteProject;
use ui::SharedString;

struct MockServer {
    host: &'static str,
    nickname: Option<&'static str>,
    project_paths: &'static [&'static str],
}

fn mock(host: &'static str, project_paths: &'static [&'static str]) -> MockServer {
    MockServer {
        host,
        nickname: None,
        project_paths,
    }
}

fn mock_with_nickname(
    host: &'static str,
    nickname: &'static str,
    project_paths: &'static [&'static str],
) -> MockServer {
    MockServer {
        host,
        nickname: Some(nickname),
        project_paths,
    }
}

fn build_entries(servers: &[MockServer]) -> Vec<RemoteEntry> {
    servers
        .iter()
        .map(|server| {
            if server.project_paths.is_empty() {
                RemoteEntry::SshConfig {
                    host: SharedString::from(server.host),
                }
            } else {
                let projects = server
                    .project_paths
                    .iter()
                    .map(|path| ProjectEntry {
                        project: RemoteProject {
                            paths: vec![(*path).to_string()],
                        },
                    })
                    .collect();
                let connection = Connection::Ssh(SshConnection {
                    host: server.host.to_string(),
                    nickname: server.nickname.map(str::to_string),
                    projects: server
                        .project_paths
                        .iter()
                        .map(|p| RemoteProject {
                            paths: vec![(*p).to_string()],
                        })
                        .collect(),
                    ..Default::default()
                });
                RemoteEntry::Project {
                    projects,
                    connection,
                    index: ServerIndex::Ssh(SshServerIndex(0)),
                }
            }
        })
        .collect()
}

fn with_filter_data<R>(
    servers: &[MockServer],
    f: impl FnOnce(&[MockServer], &FilterData) -> R,
) -> R {
    let entries = build_entries(servers);
    let data = FilterData::build(&entries);
    f(servers, &data)
}

#[test]
fn test_filter_host_only() {
    with_filter_data(&[mock("myhost", &[])], |_, data| {
        let results = run_sync(data, "myh");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].server_index, 0);
        assert!(!results[0].host_positions.is_empty());
    });
}

#[test]
fn test_filter_no_match() {
    with_filter_data(&[mock("myhost", &["/home/project"])], |_, data| {
        let results = run_sync(data, "zzz");
        assert!(results.is_empty());
    });
}

#[test]
fn test_filter_project_path_match() {
    with_filter_data(&[mock("myhost", &["/home/user/project"])], |_, data| {
        let results = run_sync(data, "project");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_matches.len(), 1);
        assert_eq!(results[0].project_matches[0].project_index, 0);
    });
}

#[test]
fn test_filter_host_match_includes_all_projects() {
    with_filter_data(&[mock("myhost", &["/path/a", "/path/b"])], |_, data| {
        let results = run_sync(data, "myhost");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_matches.len(), 2);
    });
}

#[test]
fn test_filter_excludes_non_matching_servers() {
    with_filter_data(
        &[mock("alpha", &["/path/a"]), mock("beta", &["/path/b"])],
        |_, data| {
            let results = run_sync(data, "alpha");
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].server_index, 0);
        },
    );
}

#[test]
fn test_position_mapping_splits_host_and_path() {
    with_filter_data(&[mock("dev", &["/src/app"])], |servers, data| {
        let results = run_sync(data, "dev app");

        assert_eq!(results.len(), 1);
        let result = &results[0];
        let host = servers[result.server_index].host;
        let path = servers[result.server_index].project_paths[0];

        assert!(
            result.host_positions.iter().all(|&p| p < host.len()),
            "host positions {:?} must be within host {:?} (len {})",
            result.host_positions,
            host,
            host.len(),
        );

        assert_eq!(result.project_matches.len(), 1);
        let proj = &result.project_matches[0];
        assert_eq!(proj.project_index, 0);
        assert!(
            proj.path_positions.iter().all(|&p| p < path.len()),
            "path positions {:?} must be within path {:?} (len {})",
            proj.path_positions,
            path,
            path.len(),
        );

        assert!(
            !result.host_positions.is_empty(),
            "query 'dev' should match host 'dev'"
        );
        assert!(
            !proj.path_positions.is_empty(),
            "query 'app' should match path '/src/app'"
        );
    });
}

#[test]
fn test_position_mapping_host_only_server() {
    with_filter_data(&[mock("myhost", &[])], |servers, data| {
        let results = run_sync(data, "myh");
        assert_eq!(results.len(), 1);
        let host = servers[0].host;
        assert!(
            results[0].host_positions.iter().all(|&p| p < host.len()),
            "host positions {:?} out of bounds for {:?}",
            results[0].host_positions,
            host,
        );
        assert!(results[0].project_matches.is_empty());
    });
}

#[test]
fn test_unicode_host_and_path_positions() {
    with_filter_data(&[mock("señor", &["/código/app"])], |servers, data| {
        let results = run_sync(data, "señ app");
        assert_eq!(results.len(), 1);
        let result = &results[0];
        let host = servers[0].host;
        let path = servers[0].project_paths[0];

        assert!(
            result
                .host_positions
                .iter()
                .all(|&p| p < host.len() && host.is_char_boundary(p)),
            "host positions {:?} must be valid char boundaries in {:?}",
            result.host_positions,
            host,
        );

        assert_eq!(result.project_matches.len(), 1);
        let proj = &result.project_matches[0];
        assert!(
            proj.path_positions
                .iter()
                .all(|&p| p < path.len() && path.is_char_boundary(p)),
            "path positions {:?} must be valid char boundaries in {:?}",
            proj.path_positions,
            path,
        );
    });
}

#[test]
fn test_filter_data_build_from_real_entries() {
    with_filter_data(&[mock("alpha", &[]), mock("beta", &[])], |_, data| {
        assert_eq!(data.server_count, 2);
        assert_eq!(data.candidates.len(), 2);
        assert_eq!(data.candidates[0].string, "alpha");
        assert_eq!(data.candidates[1].string, "beta");

        let results = run_sync(data, "alp");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].server_index, 0);
        assert!(!results[0].host_positions.is_empty());

        let empty = run_sync(data, "zzz");
        assert!(empty.is_empty());
    });
}

#[gpui::test]
async fn test_run_async_returns_none_when_cancelled(cx: &mut gpui::TestAppContext) {
    let data = FilterData::build(&build_entries(&[mock("alpha", &[])]));
    let cancel = AtomicBool::new(true);
    let executor = cx.background_executor.clone();
    let result = run_async(&data, "alpha", &cancel, executor).await;
    assert!(
        result.is_none(),
        "cancel set before run should short-circuit"
    );
}

#[gpui::test]
async fn test_run_async_returns_results_when_not_cancelled(cx: &mut gpui::TestAppContext) {
    let data = FilterData::build(&build_entries(&[mock("alpha", &["/home/project"])]));
    let cancel = AtomicBool::new(false);
    let executor = cx.background_executor.clone();
    let results = run_async(&data, "alpha", &cancel, executor)
        .await
        .expect("uncancelled run should return results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].server_index, 0);
    assert!(!results[0].host_positions.is_empty());
}

#[test]
fn test_filter_matches_nickname_and_host() {
    let servers = [mock_with_nickname("10.0.0.5", "prod", &["/srv/app"])];
    with_filter_data(&servers, |servers, data| {
        let nickname = servers[0].nickname.expect("server has a nickname");

        let by_nickname = run_sync(data, "prod");
        assert_eq!(by_nickname.len(), 1, "nickname should match");
        assert!(
            !by_nickname[0].host_positions.is_empty(),
            "matching the nickname should highlight it"
        );
        assert!(
            by_nickname[0]
                .host_positions
                .iter()
                .all(|&p| p < nickname.len()),
            "host positions {:?} must stay within the displayed nickname {:?}",
            by_nickname[0].host_positions,
            nickname,
        );

        let by_host = run_sync(data, "10.0");
        assert_eq!(by_host.len(), 1, "real host should remain searchable");
        assert!(
            by_host[0].host_positions.is_empty(),
            "alias-only matches are searchable but not highlighted, got {:?}",
            by_host[0].host_positions,
        );
    });
}

#[test]
fn test_projects_ordered_by_match_score() {
    with_filter_data(&[mock("srv", &["/a", "/b"])], |_, data| {
        // candidate 0 -> project 0, candidate 1 -> project 1; feed them
        // in descending-score order as `match_strings` would, then check
        // the regrouping keeps the higher-scored project first.
        let matches = vec![
            StringMatch {
                candidate_id: 1,
                score: 0.9,
                positions: Vec::new(),
                string: SharedString::default(),
            },
            StringMatch {
                candidate_id: 0,
                score: 0.5,
                positions: Vec::new(),
                string: SharedString::default(),
            },
        ];
        let results = build_filter_results(matches, data);
        assert_eq!(results.len(), 1);
        let project_indices: Vec<_> = results[0]
            .project_matches
            .iter()
            .map(|p| p.project_index)
            .collect();
        assert_eq!(project_indices, vec![1, 0]);
    });
}
