use super::*;

pub const MAX_SKILL_NAME_LEN: usize = 64;

/// Maximum recommended length (in bytes) for a skill description. The
/// create-skill UI enforces this as a hard limit, while the loader emits a
/// warning and still loads longer descriptions.
///
/// Byte-based rather than char-based because that's what `.len()` returns
/// and what every caller currently measures; the UI also surfaces this
/// limit as a byte count so the editor's counter matches the validator.
pub const MAX_SKILL_DESCRIPTION_LEN: usize = 1024;

/// Convert an arbitrary human-readable string into a valid skill name, or
/// return `None` if no valid name can be produced (e.g. the input contains
/// no ASCII alphanumeric characters at all).
///
/// The transformation:
///
/// 1. Replaces each `&` with the word `and` (with separators on either
///    side), so titles like "rock & roll" or "AT&T" round-trip something
///    meaningful (`rock-and-roll`, `at-and-t`) rather than dropping the
///    `&` and silently mashing the neighbours together.
/// 2. ASCII-lowercases every ASCII letter.
/// 3. Replaces each space with `-`. Existing `-` characters are kept.
/// 4. **Drops** every other non-alphanumeric character entirely (NOT
///    replaced with a dash). So `foo!bar` slugifies to `foobar`, not
///    `foo-bar` — only word boundaries the user actually wrote (spaces)
///    become dashes.
/// 5. Collapses runs of `-` into a single `-`.
/// 6. Trims leading and trailing `-`.
/// 7. Truncates to [`MAX_SKILL_NAME_LEN`] bytes (then re-trims trailing `-`
///    in case the truncation landed on one).
///
/// The result, if `Some`, always satisfies [`validate_name`].
pub fn slugify_skill_name(input: &str) -> Option<String> {
    // Substitute `&` with `-and-` BEFORE the per-character pass; the
    // existing dash-collapsing and edge-trimming logic then handles the
    // boundary cases (`foo & bar`, `&foo`, `foo&`, `&&`, etc.) for free.
    let input = input.replace('&', "-and-");
    let mut slug = String::with_capacity(input.len());
    let mut last_was_dash = true; // suppress a leading `-`
    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch == ' ' || ch == '-' {
            Some('-')
        } else {
            // Drop the character entirely — and importantly, do NOT touch
            // `last_was_dash`. That way `foo!bar` stays one run of
            // alphanumerics (`foobar`) rather than getting a fake
            // separator inserted (`foo-bar`).
            None
        };
        let Some(c) = mapped else { continue };
        if c == '-' {
            if last_was_dash {
                continue;
            }
            last_was_dash = true;
        } else {
            last_was_dash = false;
        }
        slug.push(c);
    }
    if slug.ends_with('-') {
        slug.pop();
    }
    if slug.len() > MAX_SKILL_NAME_LEN {
        slug.truncate(MAX_SKILL_NAME_LEN);
        while slug.ends_with('-') {
            slug.pop();
        }
    }
    if slug.is_empty() { None } else { Some(slug) }
}

/// Validate a skill name against the rules enforced by both the loader
/// and the create-skill UI.
///
/// Rules:
/// * non-empty
/// * at most [`MAX_SKILL_NAME_LEN`] bytes
/// * ASCII lowercase letters, digits, and hyphens only
/// * must not start or end with a hyphen — [`slugify_skill_name`]
///   already guarantees this for its output, so requiring it in the
///   validator keeps hand-written `SKILL.md` files consistent with
///   slugifier output
///
/// Error messages are returned as `&'static str` (interpolated at
/// compile time via `formatcp!`) so that UI surfaces can store them in
/// `Option<&'static str>` fields without allocating, and loader callers
/// can convert them to `anyhow::Error` via `anyhow::Error::msg`.
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("Skill name cannot be empty");
    }
    if name.len() > MAX_SKILL_NAME_LEN {
        return Err(formatcp!(
            "Skill name must be at most {MAX_SKILL_NAME_LEN} characters"
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("Skill name must not start or end with a hyphen");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("Skill name must contain only lowercase letters, numbers, and hyphens");
    }
    Ok(())
}

/// Validate a skill description against the strict rules enforced by the
/// create-skill UI and imported/shared skill parsing.
pub fn validate_description(description: &str) -> Result<(), &'static str> {
    if description.trim().is_empty() {
        return Err("Skill description cannot be empty");
    }
    if description.len() > MAX_SKILL_DESCRIPTION_LEN {
        return Err(formatcp!(
            "Skill description must be at most {MAX_SKILL_DESCRIPTION_LEN} bytes"
        ));
    }
    Ok(())
}
