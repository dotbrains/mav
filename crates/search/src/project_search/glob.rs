pub(super) fn split_glob_patterns(text: &str) -> Vec<&str> {
    let mut patterns = Vec::new();
    let mut pattern_start = 0;
    let mut brace_depth: usize = 0;
    let mut escaped = false;

    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if brace_depth == 0 => {
                patterns.push(&text[pattern_start..index]);
                pattern_start = index + 1;
            }
            _ => {}
        }
    }
    patterns.push(&text[pattern_start..]);
    patterns
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_split_glob_patterns() {
        assert_eq!(split_glob_patterns("a,b,c"), vec!["a", "b", "c"]);
        assert_eq!(split_glob_patterns("a, b, c"), vec!["a", " b", " c"]);
        assert_eq!(
            split_glob_patterns("src/{a,b}/**/*.rs"),
            vec!["src/{a,b}/**/*.rs"]
        );
        assert_eq!(
            split_glob_patterns("src/{a,b}/*.rs, tests/**/*.rs"),
            vec!["src/{a,b}/*.rs", " tests/**/*.rs"]
        );
        assert_eq!(split_glob_patterns("{a,b},{c,d}"), vec!["{a,b}", "{c,d}"]);
        assert_eq!(split_glob_patterns("{{a,b},{c,d}}"), vec!["{{a,b},{c,d}}"]);
        assert_eq!(split_glob_patterns(""), vec![""]);
        assert_eq!(split_glob_patterns("a"), vec!["a"]);
        assert_eq!(split_glob_patterns(r"a\,b,c"), vec![r"a\,b", "c"]);
        assert_eq!(split_glob_patterns(r"\{a,b\}"), vec![r"\{a", r"b\}"]);
        assert_eq!(split_glob_patterns(r"a\\,b"), vec![r"a\\", "b"]);
        assert_eq!(split_glob_patterns(r"a\\\,b"), vec![r"a\\\,b"]);
    }
}
