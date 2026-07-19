use super::*;

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_git_log_uses_no_pager() {
    eval_utils::eval(100, 0.95, eval_utils::NoProcessor, move || {
        run_eval(EvalInput::new(
            vec![message(
                User,
                [text(indoc::indoc! {"
                    Use the terminal tool to show me the most recent 3 commits
                    on the current branch (subject lines only is fine).
                "})],
            )],
            CommandAssertion::git_pty_safe(
                "`git log`-style prompt produces a pty-safe git command",
            ),
        ))
    });
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_git_rebase_sets_git_editor() {
    eval_utils::eval(100, 0.95, eval_utils::NoProcessor, move || {
        run_eval(EvalInput::new(
            vec![message(
                User,
                [text(indoc::indoc! {"
                    Use the terminal tool to rebase the current branch onto
                    `origin/main`.
                "})],
            )],
            CommandAssertion::git_pty_safe("`git rebase` prompt produces a pty-safe git command"),
        ))
    });
}

#[test]
#[cfg_attr(not(feature = "unit-eval"), ignore)]
fn eval_git_rebase_implied_sets_git_editor() {
    eval_utils::eval(100, 0.95, eval_utils::NoProcessor, move || {
        run_eval(EvalInput::new(
            vec![message(
                User,
                [text(indoc::indoc! {"
                    My branch has 3 small commits that I'd like to combine
                    into a single clean commit before merging. Help me do
                    that with the terminal tool.
                "})],
            )],
            CommandAssertion::git_pty_safe("indirect prompt produces a pty-safe git command"),
        ))
    });
}
