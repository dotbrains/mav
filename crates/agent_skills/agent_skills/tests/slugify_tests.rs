use super::*;

#[test]
fn test_slugify_basic() {
    assert_eq!(
        slugify_skill_name("My Cool Skill").as_deref(),
        Some("my-cool-skill")
    );
}

#[test]
fn test_slugify_strips_invalid_chars() {
    // Punctuation is dropped; spaces between words still produce dashes.
    // `Hello,` → `hello`, then `␣` → `-`, then `World!` → `world`, etc.
    assert_eq!(
        slugify_skill_name("Hello, World! (v2)").as_deref(),
        Some("hello-world-v2")
    );
}

#[test]
fn test_slugify_drops_punctuation_in_middle_no_spaces() {
    // Punctuation between alphanumerics is dropped entirely — it does
    // NOT become a dash. Only user-written spaces become dashes.
    assert_eq!(slugify_skill_name("foo!bar").as_deref(), Some("foobar"));
    assert_eq!(slugify_skill_name("foo?bar").as_deref(), Some("foobar"));
    assert_eq!(slugify_skill_name("foo%bar").as_deref(), Some("foobar"));
    assert_eq!(slugify_skill_name("100%sure").as_deref(), Some("100sure"));
    assert_eq!(
        slugify_skill_name("what's that").as_deref(),
        Some("whats-that")
    );
    // `&` is special-cased to become `and` — see
    // `test_slugify_ampersand_becomes_and` for the full coverage.
    assert_eq!(
        slugify_skill_name("don't&won't").as_deref(),
        Some("dont-and-wont")
    );
}

#[test]
fn test_slugify_ampersand_becomes_and() {
    // No spaces around `&`.
    assert_eq!(
        slugify_skill_name("foo&bar").as_deref(),
        Some("foo-and-bar")
    );
    assert_eq!(
        slugify_skill_name("rock&roll").as_deref(),
        Some("rock-and-roll")
    );
    // Spaces around `&`: collapses to a single dash on each side.
    assert_eq!(
        slugify_skill_name("foo & bar").as_deref(),
        Some("foo-and-bar")
    );
    // Asymmetric spacing.
    assert_eq!(
        slugify_skill_name("foo& bar").as_deref(),
        Some("foo-and-bar")
    );
    assert_eq!(
        slugify_skill_name("foo &bar").as_deref(),
        Some("foo-and-bar")
    );
    // Leading/trailing `&`: the substituted spaces become leading/
    // trailing dashes which then get trimmed.
    assert_eq!(slugify_skill_name("&foo").as_deref(), Some("and-foo"));
    assert_eq!(slugify_skill_name("foo&").as_deref(), Some("foo-and"));
    // `&` alone slugifies to the word `and`, not to `None`.
    assert_eq!(slugify_skill_name("&").as_deref(), Some("and"));
    assert_eq!(slugify_skill_name(" & ").as_deref(), Some("and"));
    // Multiple `&`s with various spacing all collapse properly.
    assert_eq!(slugify_skill_name("&&").as_deref(), Some("and-and"));
    assert_eq!(
        slugify_skill_name("foo & & bar").as_deref(),
        Some("foo-and-and-bar")
    );
    // Mixed with other punctuation (other punctuation is still dropped).
    assert_eq!(slugify_skill_name("AT&T").as_deref(), Some("at-and-t"));
    assert_eq!(slugify_skill_name("Q&A!").as_deref(), Some("q-and-a"));
}

#[test]
fn test_slugify_punctuation_surrounded_by_spaces() {
    // `foo ! bar` → `foo-bar`: the two spaces would each produce a
    // dash, but consecutive dashes are collapsed.
    assert_eq!(slugify_skill_name("foo ! bar").as_deref(), Some("foo-bar"));
    assert_eq!(slugify_skill_name("foo ? bar").as_deref(), Some("foo-bar"));
    assert_eq!(
        slugify_skill_name("100 % sure").as_deref(),
        Some("100-sure")
    );
    assert_eq!(
        slugify_skill_name("foo @ bar @ baz").as_deref(),
        Some("foo-bar-baz")
    );
}

#[test]
fn test_slugify_punctuation_adjacent_to_space() {
    // `foo! bar` and `foo !bar` both produce `foo-bar` — the
    // punctuation contributes nothing, the single space contributes
    // the dash.
    assert_eq!(slugify_skill_name("foo! bar").as_deref(), Some("foo-bar"));
    assert_eq!(slugify_skill_name("foo !bar").as_deref(), Some("foo-bar"));
    assert_eq!(slugify_skill_name("foo? bar").as_deref(), Some("foo-bar"));
}

#[test]
fn test_slugify_leading_and_trailing_punctuation() {
    // Punctuation at the edges is dropped; there's no leading/trailing
    // dash to trim because the punctuation never became a dash in the
    // first place.
    assert_eq!(slugify_skill_name("!foo").as_deref(), Some("foo"));
    assert_eq!(slugify_skill_name("foo!").as_deref(), Some("foo"));
    assert_eq!(slugify_skill_name("!!!foo!!!").as_deref(), Some("foo"));
    assert_eq!(slugify_skill_name("?foo?").as_deref(), Some("foo"));
    assert_eq!(slugify_skill_name("...foo...").as_deref(), Some("foo"));
}

#[test]
fn test_slugify_only_punctuation_returns_none() {
    assert_eq!(slugify_skill_name("!!!"), None);
    assert_eq!(slugify_skill_name("?@$"), None);
    assert_eq!(slugify_skill_name("()[]{}"), None);
    assert_eq!(slugify_skill_name(".,;:"), None);
}

#[test]
fn test_slugify_mixed_punctuation_spaces_and_dashes() {
    // A messy realistic input: combination of punctuation, spaces,
    // existing dashes, and casing.
    assert_eq!(
        slugify_skill_name("  -- Hello, World!! -- ").as_deref(),
        Some("hello-world")
    );
    assert_eq!(
        slugify_skill_name("C++ vs. Rust?").as_deref(),
        Some("c-vs-rust")
    );
    assert_eq!(
        slugify_skill_name("v1.2.3-beta").as_deref(),
        Some("v123-beta")
    );
}

#[test]
fn test_slugify_underscores_are_dropped() {
    // Underscores aren't a valid skill-name character and aren't
    // separators — only spaces become dashes — so underscores get
    // dropped entirely.
    assert_eq!(slugify_skill_name("foo_bar").as_deref(), Some("foobar"));
    assert_eq!(slugify_skill_name("FOO_BAR").as_deref(), Some("foobar"));
    assert_eq!(
        slugify_skill_name("snake_case style").as_deref(),
        Some("snakecase-style")
    );
}

#[test]
fn test_slugify_collapses_consecutive_dashes() {
    assert_eq!(
        slugify_skill_name("foo   ---  bar").as_deref(),
        Some("foo-bar")
    );
}

#[test]
fn test_slugify_trims_leading_and_trailing_dashes() {
    assert_eq!(slugify_skill_name("---foo---").as_deref(), Some("foo"));
    assert_eq!(slugify_skill_name("  foo  ").as_deref(), Some("foo"));
}

#[test]
fn test_slugify_lowercases() {
    assert_eq!(slugify_skill_name("FOO BAR").as_deref(), Some("foo-bar"));
    assert_eq!(
        slugify_skill_name("MyCoolSkill").as_deref(),
        Some("mycoolskill")
    );
}

#[test]
fn test_slugify_strips_non_ascii_letters() {
    // Non-ASCII chars are replaced with `-`, then collapsed.
    assert_eq!(slugify_skill_name("abc\u{00e9}").as_deref(), Some("abc"));
    assert_eq!(slugify_skill_name("\u{4e2d}\u{6587}"), None);
}

#[test]
fn test_slugify_returns_none_for_empty_or_unmappable() {
    assert_eq!(slugify_skill_name(""), None);
    assert_eq!(slugify_skill_name("   "), None);
    assert_eq!(slugify_skill_name("!!!"), None);
    assert_eq!(slugify_skill_name("---"), None);
}

#[test]
fn test_slugify_truncates_long_inputs() {
    let input = "a".repeat(200);
    let slug = slugify_skill_name(&input).expect("should slugify");
    assert_eq!(slug.len(), MAX_SKILL_NAME_LEN);
    assert!(slug.chars().all(|c| c == 'a'));
}

#[test]
fn test_slugify_truncation_does_not_leave_trailing_dash() {
    // The 64th byte lands on a `-`, which we must strip post-truncation.
    let mut input = "a".repeat(63);
    input.push_str(" extra");
    let slug = slugify_skill_name(&input).expect("should slugify");
    assert!(!slug.ends_with('-'));
    assert!(slug.len() <= MAX_SKILL_NAME_LEN);
}

#[test]
fn test_slugify_output_passes_validate_name() {
    for input in [
        "My Cool Skill",
        "Hello, World!",
        "---foo---",
        "123 abc",
        "a".repeat(200).as_str(),
    ] {
        let slug = slugify_skill_name(input).expect("should slugify");
        validate_name(&slug)
            .unwrap_or_else(|err| panic!("slug {slug:?} from {input:?} failed validation: {err}"));
    }
}
