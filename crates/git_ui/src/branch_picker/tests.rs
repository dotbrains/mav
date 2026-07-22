mod tests {
    use std::collections::HashSet;

    use super::*;
    use git::repository::{
        CommitSummary, Remote, Upstream, UpstreamTracking, UpstreamTrackingStatus,
    };
    use gpui::{AppContext, TestAppContext, VisualTestContext};
    use project::{FakeFs, Project};
    use rand::{Rng, rngs::StdRng};
    use serde_json::json;
    use settings::SettingsStore;
    use util::path;
    use workspace::MultiWorkspace;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
            editor::init(cx);
        });
    }

    fn create_test_branch(
        name: &str,
        is_head: bool,
        remote_name: Option<&str>,
        timestamp: Option<i64>,
    ) -> Branch {
        create_test_branch_with_upstream(name, is_head, remote_name, timestamp, None)
    }

    fn create_test_branch_with_upstream(
        name: &str,
        is_head: bool,
        remote_name: Option<&str>,
        timestamp: Option<i64>,
        upstream_ref_name: Option<&str>,
    ) -> Branch {
        let ref_name = match remote_name {
            Some(remote_name) => format!("refs/remotes/{remote_name}/{name}"),
            None => format!("refs/heads/{name}"),
        };

        Branch {
            is_head,
            ref_name: ref_name.into(),
            upstream: upstream_ref_name.map(|ref_name| Upstream {
                ref_name: ref_name.into(),
                tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
                    ahead: 0,
                    behind: 0,
                }),
            }),
            most_recent_commit: timestamp.map(|ts| CommitSummary {
                sha: "abc123".into(),
                commit_timestamp: ts,
                author_name: "Test Author".into(),
                subject: "Test commit".into(),
                has_parent: true,
            }),
        }
    }

    fn create_test_branches() -> Vec<Branch> {
        vec![
            create_test_branch("main", true, None, Some(1000)),
            create_test_branch("feature-auth", false, None, Some(900)),
            create_test_branch("feature-ui", false, None, Some(800)),
            create_test_branch("develop", false, None, Some(700)),
        ]
    }

    #[path = "delete.rs"]
    mod delete;
    #[path = "filter_create.rs"]
    mod filter_create;
    #[path = "remote.rs"]
    mod remote;
    #[path = "selection.rs"]
    mod selection;
}
