use super::*;

#[test]
fn test_split_leading_icon_char() {
    // A leading symbol set off by whitespace is pulled out and trimmed from the
    // title.
    let (icon, title, positions) =
        split_leading_icon_char(&"✳ Implement separate config".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "✳");
    assert_eq!(title.as_ref(), "Implement separate config");
    assert_eq!(positions, Vec::<usize>::new());

    // No prefix when the title starts with a letter.
    assert!(split_leading_icon_char(&"Implement separate config".into(), &[]).is_none());

    // Leading whitespace is not treated as a prefix.
    assert!(split_leading_icon_char(&" leading space".into(), &[]).is_none());

    // An alphanumeric prefix such as a version marker is not treated as an icon.
    assert!(split_leading_icon_char(&"v1 Running".into(), &[]).is_none());
    assert!(split_leading_icon_char(&"1 first".into(), &[]).is_none());

    // A title consisting only of a symbol (no whitespace separator) is left
    // untouched.
    assert!(split_leading_icon_char(&"✳".into(), &[]).is_none());
    assert!(split_leading_icon_char(&"✳Thinking".into(), &[]).is_none());

    // A run of the same symbol collapses to a single glyph.
    let (icon, title, _) = split_leading_icon_char(&">>> Thinking".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), ">");
    assert_eq!(title.as_ref(), "Thinking");

    // Surrounding ASCII brackets are stripped so the inner glyph is used.
    let (icon, title, _) = split_leading_icon_char(&"[!] codex waiting".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "!");
    assert_eq!(title.as_ref(), "codex waiting");

    // A run of dots is condensed into an ellipsis.
    let (icon, title, _) = split_leading_icon_char(&"... working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    let (icon, title, _) = split_leading_icon_char(&"[...] working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    let (icon, title, _) = split_leading_icon_char(&"[…] working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    // Multi-codepoint emoji are kept intact rather than sliced mid-cluster.
    let (icon, title, _) = split_leading_icon_char(&"🇺🇸 flag".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "🇺🇸");
    assert_eq!(title.as_ref(), "flag");

    // Highlight positions are shifted to account for the stripped prefix, and
    // positions that fall inside the stripped prefix are dropped.
    let title: SharedString = "# abc".into();
    let abc_offset = title.find('a').unwrap();
    let (icon, trimmed, positions) =
        split_leading_icon_char(&title, &[0, abc_offset, abc_offset + 1]).unwrap();
    assert_eq!(icon.as_ref(), "#");
    assert_eq!(trimmed.as_ref(), "abc");
    assert_eq!(positions, vec![0, 1]);
}
