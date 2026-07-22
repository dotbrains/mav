use crate::AutoCompactThreshold;
use crate::layout::parse_auto_compact_threshold;

#[test]
fn test_parse_auto_compact_threshold() {
    use AutoCompactThreshold::*;

    assert_eq!(
        parse_auto_compact_threshold("90%").unwrap(),
        Percentage(0.9)
    );
    assert_eq!(AutoCompactThreshold::DEFAULT, Percentage(0.9));
    assert_eq!(
        parse_auto_compact_threshold("  92.5% ").unwrap(),
        Percentage(0.925)
    );
    assert_eq!(
        parse_auto_compact_threshold("95.5%").unwrap(),
        Percentage(0.955)
    );
    assert_eq!(
        parse_auto_compact_threshold("100%").unwrap(),
        Percentage(1.0)
    );
    // Token counts must be integers; a non-integer token value is invalid.
    assert!(parse_auto_compact_threshold("100.5").is_err());
    assert_eq!(
        parse_auto_compact_threshold("100000").unwrap(),
        TokensUsed(100_000)
    );
    assert_eq!(
        parse_auto_compact_threshold("-20000").unwrap(),
        TokensRemaining(20_000)
    );

    assert_eq!(Percentage(0.9).to_string(), "90%");
    assert_eq!(Percentage(0.925).to_string(), "92.5%");
    assert_eq!(TokensUsed(100_000).to_string(), "100000");
    assert_eq!(TokensRemaining(20_000).to_string(), "-20000");

    // 0 is invalid in every form.
    assert!(parse_auto_compact_threshold("0").is_err());
    assert!(parse_auto_compact_threshold("0%").is_err());
    // Out-of-range percentages and bare decimals are invalid.
    assert!(parse_auto_compact_threshold("150%").is_err());
    assert!(parse_auto_compact_threshold("0.8").is_err());
    assert!(parse_auto_compact_threshold("eighty percent").is_err());
}
