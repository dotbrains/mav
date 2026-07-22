use super::super::*;
use ui::utils::apca_contrast;

#[test]
fn test_contrast_adjustment_logic() {
    // Test the core contrast adjustment logic without needing full app context

    // Test case 1: Light colors (poor contrast)
    let white_fg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };
    let light_gray_bg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.95,
        a: 1.0,
    };

    // Should have poor contrast
    let actual_contrast = apca_contrast(white_fg, light_gray_bg).abs();
    assert!(
        actual_contrast < 30.0,
        "White on light gray should have poor APCA contrast: {}",
        actual_contrast
    );

    // After adjustment with minimum APCA contrast of 45, should be darker
    let adjusted = ensure_minimum_contrast(white_fg, light_gray_bg, 45.0);
    assert!(
        adjusted.l < white_fg.l,
        "Adjusted color should be darker than original"
    );
    let adjusted_contrast = apca_contrast(adjusted, light_gray_bg).abs();
    assert!(adjusted_contrast >= 45.0, "Should meet minimum contrast");

    // Test case 2: Dark colors (poor contrast)
    let black_fg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 1.0,
    };
    let dark_gray_bg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.05,
        a: 1.0,
    };

    // Should have poor contrast
    let actual_contrast = apca_contrast(black_fg, dark_gray_bg).abs();
    assert!(
        actual_contrast < 30.0,
        "Black on dark gray should have poor APCA contrast: {}",
        actual_contrast
    );

    // After adjustment with minimum APCA contrast of 45, should be lighter
    let adjusted = ensure_minimum_contrast(black_fg, dark_gray_bg, 45.0);
    assert!(
        adjusted.l > black_fg.l,
        "Adjusted color should be lighter than original"
    );
    let adjusted_contrast = apca_contrast(adjusted, dark_gray_bg).abs();
    assert!(adjusted_contrast >= 45.0, "Should meet minimum contrast");

    // Test case 3: Already good contrast
    let good_contrast = ensure_minimum_contrast(black_fg, white_fg, 45.0);
    assert_eq!(
        good_contrast, black_fg,
        "Good contrast should not be adjusted"
    );
}

#[test]
fn test_true_color_red_blue_not_washed_out_on_dark_bg() {
    // Red and blue have inherently low perceptual luminance in APCA.
    // Pure #ff0000 only achieves Lc ~35 against #1e1e1e — below the
    // default Lc 45 threshold. ensure_minimum_contrast would lighten
    // them, washing out the color. This is why cell_style skips the
    // adjustment for Color::Spec (24-bit true color).
    let dark_bg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.05,
        a: 1.0,
    };

    for (name, r, g, b) in [
        ("red", 225, 80, 80),
        ("blue", 80, 80, 225),
        ("pure red", 255, 0, 0),
    ] {
        let color = terminal::rgba_color(r, g, b);
        let contrast = apca_contrast(color, dark_bg).abs();
        assert!(
            contrast < 45.0,
            "{name} should have APCA < 45 on dark bg, got {contrast}",
        );

        let adjusted = ensure_minimum_contrast(color, dark_bg, 45.0);
        assert!(
            adjusted.l > color.l,
            "{name} would be lightened by contrast adjustment (l: {} -> {})",
            color.l,
            adjusted.l,
        );
    }
}

#[test]
fn test_white_on_white_contrast_issue() {
    // This test reproduces the exact issue from the bug report
    // where white ANSI text on white background should be adjusted

    // Simulate One Light theme colors
    let white_fg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.98, // #fafafaff is approximately 98% lightness
        a: 1.0,
    };
    let white_bg = gpui::Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.98, // Same as foreground - this is the problem!
        a: 1.0,
    };

    // With minimum contrast of 0.0, no adjustment should happen
    let no_adjust = ensure_minimum_contrast(white_fg, white_bg, 0.0);
    assert_eq!(no_adjust, white_fg, "No adjustment with min_contrast 0.0");

    // With minimum APCA contrast of 15, it should adjust to a darker color
    let adjusted = ensure_minimum_contrast(white_fg, white_bg, 15.0);
    assert!(
        adjusted.l < white_fg.l,
        "White on white should become darker, got l={}",
        adjusted.l
    );

    // Verify the contrast is now acceptable
    let new_contrast = apca_contrast(adjusted, white_bg).abs();
    assert!(
        new_contrast >= 15.0,
        "Adjusted APCA contrast {} should be >= 15.0",
        new_contrast
    );
}
