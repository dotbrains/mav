use super::*;
use pretty_assertions::assert_eq;

#[test]
fn test_prepare_empty_task() {
    let input = SpawnInTerminal::default();
    let shell = Shell::System;

    let result = prepare_task_for_spawn(&input, &shell, false);

    let expected_shell = util::get_system_shell();
    assert_eq!(result.env, HashMap::default());
    assert_eq!(result.cwd, None);
    assert_eq!(result.shell, Shell::System);
    assert_eq!(
        result.command,
        Some(expected_shell.clone()),
        "Empty tasks should spawn a -i shell"
    );
    assert_eq!(result.args, Vec::<String>::new());
    assert_eq!(
        result.command_label, expected_shell,
        "We show the shell launch for empty commands"
    );
}
#[test]
fn test_prepare_script_like_task() {
    let user_command = r#"REPO_URL=$(git remote get-url origin | sed -e \"s/^git@\\(.*\\):\\(.*\\)\\.git$/https:\\/\\/\\1\\/\\2/\"); COMMIT_SHA=$(git log -1 --format=\"%H\" -- \"${MAV_RELATIVE_FILE}\"); echo \"${REPO_URL}/blob/${COMMIT_SHA}/${MAV_RELATIVE_FILE}#L${MAV_ROW}-$(echo $(($(wc -l <<< \"$MAV_SELECTED_TEXT\") + $MAV_ROW - 1)))\" | xclip -selection clipboard"#.to_string();
    let expected_cwd = PathBuf::from("/some/work");

    let input = SpawnInTerminal {
        command: Some(user_command.clone()),
        cwd: Some(expected_cwd.clone()),
        ..SpawnInTerminal::default()
    };
    let shell = Shell::System;

    let result = prepare_task_for_spawn(&input, &shell, false);

    let system_shell = util::get_system_shell();
    assert_eq!(result.env, HashMap::default());
    assert_eq!(result.cwd, Some(expected_cwd));
    assert_eq!(result.shell, Shell::System);
    assert_eq!(result.command, Some(system_shell.clone()));
    assert_eq!(
        result.args,
        vec!["-i".to_string(), "-c".to_string(), user_command.clone()],
        "User command should have been moved into the arguments, as we're spawning a new -i shell",
    );
    assert_eq!(
        result.command_label,
        format!(
            "{system_shell} {interactive}-c '{user_command}'",
            interactive = if cfg!(windows) { "" } else { "-i " }
        ),
        "We want to show to the user the entire command spawned"
    );
}
