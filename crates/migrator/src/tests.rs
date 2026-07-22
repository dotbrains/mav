use super::*;
use unindent::Unindent as _;

mod builtin_agent_servers_tests;
mod code_action_formatter_tests;
mod edit_prediction_provider_tests;
mod enum_setting_tests;
mod experimental_provider_tests;
mod format_on_save_code_action_tests;
mod formatting_tests;
mod keymap_action_tests;
mod mcp_settings_tests;
mod profile_settings_tests;
mod setting_replacement_tests;
mod settings_cleanup_tests;
mod sidebar_tests;
mod tool_permission_core_tests;
mod tool_permission_profile_tests;
mod web_search_tests;

#[track_caller]
fn assert_migrated_correctly(migrated: Option<String>, expected: Option<&str>) {
    match (&migrated, &expected) {
        (Some(migrated), Some(expected)) => {
            pretty_assertions::assert_str_eq!(expected, migrated);
        }
        _ => {
            pretty_assertions::assert_eq!(migrated.as_deref(), expected);
        }
    }
}

#[track_caller]
fn assert_migrate_keymap(input: &str, output: Option<&str>) {
    let migrated = migrate_keymap(input).unwrap();
    pretty_assertions::assert_eq!(migrated.as_deref(), output);
}

#[track_caller]
fn assert_migrate_settings(input: &str, output: Option<&str>) {
    let migrated = migrate_settings(input).unwrap();
    assert_migrated_correctly(migrated.clone(), output);

    // expect that rerunning the migration does not result in another migration
    if let Some(migrated) = migrated {
        let rerun = migrate_settings(&migrated).unwrap();
        assert_migrated_correctly(rerun, None);
    }
}

#[track_caller]
fn assert_migrate_with_migrations(migrations: &[MigrationType], input: &str, output: Option<&str>) {
    let migrated = run_migrations(input, migrations).unwrap();
    assert_migrated_correctly(migrated.clone(), output);

    // expect that rerunning the migration does not result in another migration
    if let Some(migrated) = migrated {
        let rerun = run_migrations(&migrated, migrations).unwrap();
        assert_migrated_correctly(rerun, None);
    }
}
