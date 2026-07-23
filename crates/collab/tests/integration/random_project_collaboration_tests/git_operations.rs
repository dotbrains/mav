use super::*;

pub(super) async fn apply_git_operation(
    client: &TestClient,
    operation: GitOperation,
) -> Result<(), TestError> {
    match operation {
        GitOperation::WriteGitIndex {
            repo_path,
            contents,
        } => {
            if !client.fs().directories(false).contains(&repo_path) {
                return Err(TestError::Inapplicable);
            }

            for (path, _) in contents.iter() {
                if !client
                    .fs()
                    .files()
                    .contains(&repo_path.join(path.as_std_path()))
                {
                    return Err(TestError::Inapplicable);
                }
            }

            log::info!(
                "{}: writing git index for repo {:?}: {:?}",
                client.username,
                repo_path,
                contents
            );

            let dot_git_dir = repo_path.join(".git");
            let contents = contents
                .iter()
                .map(|(path, contents)| (path.as_unix_str(), contents.clone()))
                .collect::<Vec<_>>();
            if client.fs().metadata(&dot_git_dir).await?.is_none() {
                client.fs().create_dir(&dot_git_dir).await?;
            }
            client.fs().set_index_for_repo(&dot_git_dir, &contents);
        }
        GitOperation::WriteGitBranch {
            repo_path,
            new_branch,
        } => {
            if !client.fs().directories(false).contains(&repo_path) {
                return Err(TestError::Inapplicable);
            }

            log::info!(
                "{}: writing git branch for repo {:?}: {:?}",
                client.username,
                repo_path,
                new_branch
            );

            let dot_git_dir = repo_path.join(".git");
            if client.fs().metadata(&dot_git_dir).await?.is_none() {
                client.fs().create_dir(&dot_git_dir).await?;
            }
            client
                .fs()
                .set_branch_name(&dot_git_dir, new_branch.clone());
        }
        GitOperation::WriteGitStatuses {
            repo_path,
            statuses,
        } => {
            if !client.fs().directories(false).contains(&repo_path) {
                return Err(TestError::Inapplicable);
            }
            for (path, _) in statuses.iter() {
                if !client
                    .fs()
                    .files()
                    .contains(&repo_path.join(path.as_std_path()))
                {
                    return Err(TestError::Inapplicable);
                }
            }

            log::info!(
                "{}: writing git statuses for repo {:?}: {:?}",
                client.username,
                repo_path,
                statuses
            );

            let dot_git_dir = repo_path.join(".git");

            let statuses = statuses
                .iter()
                .map(|(path, val)| (path.as_unix_str(), *val))
                .collect::<Vec<_>>();

            if client.fs().metadata(&dot_git_dir).await?.is_none() {
                client.fs().create_dir(&dot_git_dir).await?;
            }

            client
                .fs()
                .set_status_for_repo(&dot_git_dir, statuses.as_slice());
        }
    }
    Ok(())
}
