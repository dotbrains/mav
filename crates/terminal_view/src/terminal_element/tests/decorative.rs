use super::super::*;

#[test]
fn test_is_decorative_character() {
    // Box Drawing characters (U+2500 to U+257F)
    assert!(TerminalElement::is_decorative_character('─')); // U+2500
    assert!(TerminalElement::is_decorative_character('│')); // U+2502
    assert!(TerminalElement::is_decorative_character('┌')); // U+250C
    assert!(TerminalElement::is_decorative_character('┐')); // U+2510
    assert!(TerminalElement::is_decorative_character('└')); // U+2514
    assert!(TerminalElement::is_decorative_character('┘')); // U+2518
    assert!(TerminalElement::is_decorative_character('┼')); // U+253C

    // Block Elements (U+2580 to U+259F)
    assert!(TerminalElement::is_decorative_character('▀')); // U+2580
    assert!(TerminalElement::is_decorative_character('▄')); // U+2584
    assert!(TerminalElement::is_decorative_character('█')); // U+2588
    assert!(TerminalElement::is_decorative_character('░')); // U+2591
    assert!(TerminalElement::is_decorative_character('▒')); // U+2592
    assert!(TerminalElement::is_decorative_character('▓')); // U+2593

    // Geometric Shapes - block/box-like subset (U+25A0 to U+25D7)
    assert!(TerminalElement::is_decorative_character('■')); // U+25A0
    assert!(TerminalElement::is_decorative_character('□')); // U+25A1
    assert!(TerminalElement::is_decorative_character('▲')); // U+25B2
    assert!(TerminalElement::is_decorative_character('▼')); // U+25BC
    assert!(TerminalElement::is_decorative_character('◆')); // U+25C6
    assert!(TerminalElement::is_decorative_character('●')); // U+25CF

    // The specific character from the issue
    assert!(TerminalElement::is_decorative_character('◗')); // U+25D7
    assert!(TerminalElement::is_decorative_character('◘')); // U+25D8 (now included in Geometric Shapes)
    assert!(TerminalElement::is_decorative_character('◙')); // U+25D9 (now included in Geometric Shapes)

    // Powerline symbols (Private Use Area)
    assert!(TerminalElement::is_decorative_character('\u{E0B0}')); // Powerline right triangle
    assert!(TerminalElement::is_decorative_character('\u{E0B2}')); // Powerline left triangle
    assert!(TerminalElement::is_decorative_character('\u{E0B4}')); // Powerline right half circle (the actual issue!)
    assert!(TerminalElement::is_decorative_character('\u{E0B6}')); // Powerline left half circle
    assert!(TerminalElement::is_decorative_character('\u{E0CA}')); // Powerline mirrored ice waveform
    assert!(TerminalElement::is_decorative_character('\u{E0D7}')); // Powerline left triangle inverted

    // Characters that should NOT be considered decorative
    assert!(!TerminalElement::is_decorative_character('A')); // Regular letter
    assert!(!TerminalElement::is_decorative_character('$')); // Symbol
    assert!(!TerminalElement::is_decorative_character(' ')); // Space
    assert!(!TerminalElement::is_decorative_character('←')); // U+2190 (Arrow, not in our ranges)
    assert!(!TerminalElement::is_decorative_character('→')); // U+2192 (Arrow, not in our ranges)
    assert!(!TerminalElement::is_decorative_character('\u{F00C}')); // Font Awesome check (icon, needs contrast)
    assert!(!TerminalElement::is_decorative_character('\u{E711}')); // Devicons (icon, needs contrast)
    assert!(!TerminalElement::is_decorative_character('\u{EA71}')); // Codicons folder (icon, needs contrast)
    assert!(!TerminalElement::is_decorative_character('\u{F401}')); // Octicons (icon, needs contrast)
    assert!(!TerminalElement::is_decorative_character('\u{1F600}')); // Emoji (not in our ranges)
}

#[test]
fn test_decorative_character_boundary_cases() {
    // Test exact boundaries of our ranges
    // Box Drawing range boundaries
    assert!(TerminalElement::is_decorative_character('\u{2500}')); // First char
    assert!(TerminalElement::is_decorative_character('\u{257F}')); // Last char
    assert!(!TerminalElement::is_decorative_character('\u{24FF}')); // Just before

    // Block Elements range boundaries
    assert!(TerminalElement::is_decorative_character('\u{2580}')); // First char
    assert!(TerminalElement::is_decorative_character('\u{259F}')); // Last char

    // Geometric Shapes subset boundaries
    assert!(TerminalElement::is_decorative_character('\u{25A0}')); // First char
    assert!(TerminalElement::is_decorative_character('\u{25FF}')); // Last char
    assert!(!TerminalElement::is_decorative_character('\u{2600}')); // Just after
}

#[test]
fn test_decorative_characters_bypass_contrast_adjustment() {
    // Decorative characters should not be affected by contrast adjustment

    // The specific character from issue #34234
    let problematic_char = '◗'; // U+25D7
    assert!(
        TerminalElement::is_decorative_character(problematic_char),
        "Character ◗ (U+25D7) should be recognized as decorative"
    );

    // Verify some other commonly used decorative characters
    assert!(TerminalElement::is_decorative_character('│')); // Vertical line
    assert!(TerminalElement::is_decorative_character('─')); // Horizontal line
    assert!(TerminalElement::is_decorative_character('█')); // Full block
    assert!(TerminalElement::is_decorative_character('▓')); // Dark shade
    assert!(TerminalElement::is_decorative_character('■')); // Black square
    assert!(TerminalElement::is_decorative_character('●')); // Black circle

    // Verify normal text characters are NOT decorative
    assert!(!TerminalElement::is_decorative_character('A'));
    assert!(!TerminalElement::is_decorative_character('1'));
    assert!(!TerminalElement::is_decorative_character('$'));
    assert!(!TerminalElement::is_decorative_character(' '));
}

#[test]
fn test_is_app_chosen_exact_color() {
    use terminal::{Color, NamedColor, Rgb};

    // Indices 0..=15 are theme-overridable ANSI colors; contrast adjustment must still apply.
    assert!(!TerminalElement::is_app_chosen_exact_color(
        &Color::Indexed(0)
    ));
    assert!(!TerminalElement::is_app_chosen_exact_color(
        &Color::Indexed(15)
    ));

    // Boundary: index 16 is the first entry of the 6x6x6 cube — application-chosen.
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Indexed(
        16
    )));
    // Interior of the cube.
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Indexed(
        17
    )));
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Indexed(
        231
    )));
    // Grayscale ramp boundaries.
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Indexed(
        232
    )));
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Indexed(
        255
    )));

    // 24-bit true color is always application-chosen.
    assert!(TerminalElement::is_app_chosen_exact_color(&Color::Spec(
        Rgb {
            r: 10,
            g: 20,
            b: 30
        }
    )));

    // Named colors are theme-defined and must go through contrast adjustment.
    assert!(!TerminalElement::is_app_chosen_exact_color(&Color::Named(
        NamedColor::Red
    )));
    assert!(!TerminalElement::is_app_chosen_exact_color(&Color::Named(
        NamedColor::Foreground
    )));
}
