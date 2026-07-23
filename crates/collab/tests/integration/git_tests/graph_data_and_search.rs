use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_remote_git_graph_data_and_search(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    cx_a.update(|cx| {
        git_ui::init(cx);
    });
    cx_b.update(|cx| {
        git_ui::init(cx);
    });
    let active_call_a = cx_a.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(
            path!("/project"),
            json!({ ".git": {}, "file.txt": "content" }),
        )
        .await;

    let search_query = "graph search match";
    let mut rng = StdRng::seed_from_u64(7);
    let commits = git_ui::git_graph::generate_random_commit_dag(&mut rng, 12, true);

    let dot_git = Path::new(path!("/project/.git"));
    client_a.fs().set_graph_commits(dot_git, commits.clone());
    client_a.fs().set_commit_data(
        dot_git,
        commits.iter().enumerate().map(|(index, commit)| {
            (
                CommitData {
                    sha: commit.sha,
                    parents: commit.parents.clone(),
                    author_name: SharedString::from(format!("Author {index}")),
                    author_email: SharedString::from(format!("author{index}@example.com")),
                    commit_timestamp: 1_700_000_000 + index as i64,
                    subject: SharedString::from(format!("Subject {index}")),
                    message: SharedString::from(if index % 2 == 0 {
                        format!("Subject {index}\n\n{search_query} {index}")
                    } else {
                        format!("Subject {index}\n\nPlain message {index}")
                    }),
                },
                false,
            )
        }),
    );

    let (project_a, _) = client_a.build_local_project(path!("/project"), cx_a).await;
    executor.run_until_parked();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    executor.run_until_parked();

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let remote_graph = build_git_graph(&project_b, &workspace_b, cx_b);
    render_git_graph(&remote_graph, cx_b);
    let remote_initial_graph_data =
        remote_graph.read_with(cx_b, |graph, _| graph.initial_commit_data_for_test());
    remote_graph.update(cx_b, |graph, cx| {
        graph.search_for_test(SharedString::from(search_query), cx);
    });
    cx_b.run_until_parked();
    let remote_search_results =
        remote_graph.read_with(cx_b, |graph, _| graph.search_matches_for_test());

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let local_graph = build_git_graph(&project_a, &workspace_a, cx_a);
    render_git_graph(&local_graph, cx_a);
    let local_initial_graph_data =
        local_graph.read_with(cx_a, |graph, _| graph.initial_commit_data_for_test());
    local_graph.update(cx_a, |graph, cx| {
        graph.search_for_test(SharedString::from(search_query), cx);
    });
    cx_a.run_until_parked();
    let local_search_results =
        local_graph.read_with(cx_a, |graph, _| graph.search_matches_for_test());

    assert_initial_graph_commits_eq(&local_initial_graph_data, &commits);
    assert_initial_graph_commits_eq(&remote_initial_graph_data, &local_initial_graph_data);
    assert!(!local_search_results.is_empty());
    assert_eq!(remote_search_results, local_search_results);
}
