use super::*;

impl RealGitRepository {
    pub(super) fn repository_branches(&self) -> BoxFuture<'_, Result<BranchesScanResult>> {
        let git = self.git_binary();
        self.executor
            .spawn(async move {
                let fields = [
                    "%(HEAD)",
                    "%(objectname)",
                    "%(parent)",
                    "%(refname)",
                    "%(upstream)",
                    "%(upstream:track)",
                    "%(committerdate:unix)",
                    "%(authorname)",
                    "%(contents:subject)",
                ]
                .join("%00");
                let args = vec![
                    "for-each-ref",
                    "refs/heads/**/*",
                    "refs/remotes/**/*",
                    "--format",
                    &fields,
                ];
                let output = git.build_command(&args).output().await?;

                let error = if output.status.success() {
                    None
                } else {
                    let error = format_branch_scan_error(&output);
                    log::warn!("failed to get git branches with commit metadata: {error}");
                    Some(error.into())
                };

                let input = String::from_utf8_lossy(&output.stdout);
                let mut branches = parse_branch_input(&input)?;
                if branches.is_empty() {
                    let args = vec!["symbolic-ref", "--quiet", "HEAD"];

                    let output = git.build_command(&args).output().await?;

                    // git symbolic-ref returns a non-0 exit code if HEAD points
                    // to something other than a branch
                    if output.status.success() {
                        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();

                        branches.push(Branch {
                            ref_name: name.into(),
                            is_head: true,
                            upstream: None,
                            most_recent_commit: None,
                        });
                    }
                }

                Ok(BranchesScanResult { branches, error })
            })
            .boxed()
    }
}
