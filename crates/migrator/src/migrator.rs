//! ## When to create a migration and why?
//! A migration is necessary when keymap actions or settings are renamed or transformed (e.g., from an array to a string, a string to an array, a boolean to an enum, etc.).
//!
//! This ensures that users with outdated settings are automatically updated to use the corresponding new settings internally.
//! It also provides a quick way to migrate their existing settings to the latest state using button in UI.
//!
//! ## How to create a migration?
//! Migrations use Tree-sitter to query commonly used patterns, such as actions with a string or actions with an array where the second argument is an object, etc.
//! Once queried, *you can filter out the modified items* and write the replacement logic.
//!
//! You *must not* modify previous migrations; always create new ones instead.
//! This is important because if a user is in an intermediate state, they can smoothly transition to the latest state.
//! Modifying existing migrations means they will only work for users upgrading from version x-1 to x, but not from x-2 to x, and so on, where x is the latest version.
//!
//! You only need to write replacement logic for x-1 to x because you can be certain that, internally, every user will be at x-1, regardless of their on disk state.

use anyhow::{Context as _, Result};
use settings_json::{infer_json_indent_size, parse_json_with_comments, update_value_in_json_text};
use std::{cmp::Reverse, ops::Range, sync::LazyLock};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryMatch};

use patterns::SETTINGS_NESTED_KEY_VALUE_PATTERN;

mod migrations;
mod patterns;
#[cfg(test)]
mod tests;

fn migrate(text: &str, patterns: MigrationPatterns, query: &Query) -> Result<Option<String>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_json::LANGUAGE.into())?;
    let syntax_tree = parser
        .parse(text, None)
        .context("failed to parse settings")?;

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(query, syntax_tree.root_node(), text.as_bytes());

    let mut edits = vec![];
    while let Some(mat) = matches.next() {
        if let Some((_, callback)) = patterns.get(mat.pattern_index) {
            edits.extend(callback(text, mat, query));
        }
    }

    edits.sort_by_key(|(range, _)| (range.start, Reverse(range.end)));
    edits.dedup_by(|(range_b, _), (range_a, _)| {
        range_a.contains(&range_b.start) || range_a.contains(&range_b.end)
    });

    if edits.is_empty() {
        Ok(None)
    } else {
        let mut new_text = text.to_string();
        for (range, replacement) in edits.iter().rev() {
            new_text.replace_range(range.clone(), replacement);
        }
        if new_text == text {
            log::error!(
                "Edits computed for configuration migration do not cause a change: {:?}",
                edits
            );
            Ok(None)
        } else {
            Ok(Some(new_text))
        }
    }
}

/// Runs the provided migrations on the given text.
/// Will automatically return `Ok(None)` if there's no content to migrate.
fn run_migrations(text: &str, migrations: &[MigrationType]) -> Result<Option<String>> {
    if text.is_empty() {
        return Ok(None);
    }

    let mut current_text = text.to_string();
    let mut result: Option<String> = None;
    let json_indent_size = infer_json_indent_size(&current_text);
    for migration in migrations.iter() {
        let migrated_text = match migration {
            MigrationType::TreeSitter(patterns, query) => migrate(&current_text, patterns, query)?,
            MigrationType::Json(callback) => {
                if current_text.trim().is_empty() {
                    return Ok(None);
                }
                let old_content: serde_json_lenient::Value =
                    parse_json_with_comments(&current_text)?;
                let old_value = serde_json::to_value(&old_content).unwrap();
                let mut new_value = old_value.clone();
                callback(&mut new_value)?;
                if new_value != old_value {
                    let mut current = current_text.clone();
                    let mut edits = vec![];
                    update_value_in_json_text(
                        &mut current,
                        &mut vec![],
                        json_indent_size,
                        &old_value,
                        &new_value,
                        &mut edits,
                    );
                    let mut migrated_text = current_text.clone();
                    for (range, replacement) in edits.into_iter() {
                        migrated_text.replace_range(range, &replacement);
                    }
                    Some(migrated_text)
                } else {
                    None
                }
            }
        };
        if let Some(migrated_text) = migrated_text {
            current_text = migrated_text.clone();
            result = Some(migrated_text);
        }
    }
    Ok(result.filter(|new_text| text != new_text))
}

pub fn migrate_keymap(text: &str) -> Result<Option<String>> {
    let migrations: &[MigrationType] = &[
        MigrationType::TreeSitter(
            migrations::m_2025_01_29::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_01_29,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_01_30::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_01_30,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_03_03::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_03_03,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_03_06::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_03_06,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_04_15::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_04_15,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_12_08::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2025_12_08,
        ),
        MigrationType::TreeSitter(
            migrations::m_2026_03_23::KEYMAP_PATTERNS,
            &KEYMAP_QUERY_2026_03_23,
        ),
    ];
    run_migrations(text, migrations)
}

enum MigrationType<'a> {
    TreeSitter(MigrationPatterns, &'a Query),
    Json(fn(&mut serde_json::Value) -> Result<()>),
}

pub fn migrate_settings(text: &str) -> Result<Option<String>> {
    let migrations: &[MigrationType] = &[
        MigrationType::TreeSitter(
            migrations::m_2025_01_02::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_01_02,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_01_29::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_01_29,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_01_30::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_01_30,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_03_29::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_03_29,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_04_15::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_04_15,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_04_21::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_04_21,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_04_23::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_04_23,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_05_05::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_05_05,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_05_08::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_05_08,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_06_16::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_06_16,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_06_25::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_06_25,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_06_27::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_06_27,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_07_08::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_07_08,
        ),
        MigrationType::Json(migrations::m_2025_10_01::flatten_code_actions_formatters),
        MigrationType::Json(migrations::m_2025_10_02::remove_formatters_on_save),
        MigrationType::TreeSitter(
            migrations::m_2025_10_03::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_10_03,
        ),
        MigrationType::Json(migrations::m_2025_10_16::restore_code_actions_on_format),
        MigrationType::Json(migrations::m_2025_10_17::make_file_finder_include_ignored_an_enum),
        MigrationType::Json(migrations::m_2025_10_21::make_relative_line_numbers_an_enum),
        MigrationType::TreeSitter(
            migrations::m_2025_11_12::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_11_12,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_12_01::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_12_01,
        ),
        MigrationType::TreeSitter(
            migrations::m_2025_11_20::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_11_20,
        ),
        MigrationType::Json(migrations::m_2025_11_25::remove_context_server_source),
        MigrationType::TreeSitter(
            migrations::m_2025_12_15::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2025_12_15,
        ),
        MigrationType::Json(migrations::m_2025_01_27::make_auto_indent_an_enum),
        MigrationType::Json(
            migrations::m_2026_02_02::move_edit_prediction_provider_to_edit_predictions,
        ),
        MigrationType::Json(migrations::m_2026_02_03::migrate_experimental_sweep_mercury),
        MigrationType::Json(migrations::m_2026_02_04::migrate_tool_permission_defaults),
        MigrationType::Json(migrations::m_2026_02_25::migrate_builtin_agent_servers_to_registry),
        MigrationType::TreeSitter(
            migrations::m_2026_03_16::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2026_03_16,
        ),
        MigrationType::Json(migrations::m_2026_03_30::make_play_sound_when_agent_done_an_enum),
        MigrationType::Json(migrations::m_2026_04_01::restructure_profiles_with_settings_key),
        MigrationType::Json(migrations::m_2026_04_10::rename_web_search_to_search_web),
        MigrationType::Json(
            migrations::m_2026_04_17::promote_show_branch_icon_true_to_show_branch_status_icon,
        ),
        MigrationType::TreeSitter(
            migrations::m_2026_05_04::SETTINGS_PATTERNS,
            &SETTINGS_QUERY_2026_05_04,
        ),
    ];
    run_migrations(text, migrations)
}

pub fn migrate_edit_prediction_provider_settings(text: &str) -> Result<Option<String>> {
    migrate(
        text,
        &[(
            SETTINGS_NESTED_KEY_VALUE_PATTERN,
            migrations::m_2025_01_29::replace_edit_prediction_provider_setting,
        )],
        &EDIT_PREDICTION_SETTINGS_MIGRATION_QUERY,
    )
}

pub type MigrationPatterns = &'static [(
    &'static str,
    fn(&str, &QueryMatch, &Query) -> Option<(Range<usize>, String)>,
)];

macro_rules! define_query {
    ($var_name:ident, $patterns_path:path) => {
        static $var_name: LazyLock<Query> = LazyLock::new(|| {
            Query::new(
                &tree_sitter_json::LANGUAGE.into(),
                &$patterns_path
                    .iter()
                    .map(|pattern| pattern.0)
                    .collect::<String>(),
            )
            .unwrap()
        });
    };
}

// keymap
define_query!(
    KEYMAP_QUERY_2025_01_29,
    migrations::m_2025_01_29::KEYMAP_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2025_01_30,
    migrations::m_2025_01_30::KEYMAP_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2025_03_03,
    migrations::m_2025_03_03::KEYMAP_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2025_03_06,
    migrations::m_2025_03_06::KEYMAP_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2025_04_15,
    migrations::m_2025_04_15::KEYMAP_PATTERNS
);

// settings
define_query!(
    SETTINGS_QUERY_2025_01_02,
    migrations::m_2025_01_02::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_01_29,
    migrations::m_2025_01_29::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_01_30,
    migrations::m_2025_01_30::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_03_29,
    migrations::m_2025_03_29::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_04_15,
    migrations::m_2025_04_15::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_04_21,
    migrations::m_2025_04_21::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_04_23,
    migrations::m_2025_04_23::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_05_05,
    migrations::m_2025_05_05::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_05_08,
    migrations::m_2025_05_08::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_06_16,
    migrations::m_2025_06_16::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_06_25,
    migrations::m_2025_06_25::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_06_27,
    migrations::m_2025_06_27::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_07_08,
    migrations::m_2025_07_08::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_10_03,
    migrations::m_2025_10_03::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_11_12,
    migrations::m_2025_11_12::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_12_01,
    migrations::m_2025_12_01::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_11_20,
    migrations::m_2025_11_20::SETTINGS_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2025_12_08,
    migrations::m_2025_12_08::KEYMAP_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2025_12_15,
    migrations::m_2025_12_15::SETTINGS_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2026_03_16,
    migrations::m_2026_03_16::SETTINGS_PATTERNS
);
define_query!(
    KEYMAP_QUERY_2026_03_23,
    migrations::m_2026_03_23::KEYMAP_PATTERNS
);
define_query!(
    SETTINGS_QUERY_2026_05_04,
    migrations::m_2026_05_04::SETTINGS_PATTERNS
);

// custom query
static EDIT_PREDICTION_SETTINGS_MIGRATION_QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(
        &tree_sitter_json::LANGUAGE.into(),
        SETTINGS_NESTED_KEY_VALUE_PATTERN,
    )
    .unwrap()
});
