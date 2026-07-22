use super::super::*;
use crate::terminal_element::layout::{BackgroundRegion, merge_background_regions};
use gpui::Hsla;

#[test]
fn test_background_region_can_merge() {
    let color1 = Hsla::red();
    let color2 = Hsla::blue();

    // Test horizontal merging
    let mut region1 = BackgroundRegion::new(0, 0, color1);
    region1.end_col = 5;
    let region2 = BackgroundRegion::new(0, 6, color1);
    assert!(region1.can_merge_with(&region2));

    // Test vertical merging with same column span
    let mut region3 = BackgroundRegion::new(0, 0, color1);
    region3.end_col = 5;
    let mut region4 = BackgroundRegion::new(1, 0, color1);
    region4.end_col = 5;
    assert!(region3.can_merge_with(&region4));

    // Test cannot merge different colors
    let region5 = BackgroundRegion::new(0, 0, color1);
    let region6 = BackgroundRegion::new(0, 1, color2);
    assert!(!region5.can_merge_with(&region6));

    // Test cannot merge non-adjacent regions
    let region7 = BackgroundRegion::new(0, 0, color1);
    let region8 = BackgroundRegion::new(0, 2, color1);
    assert!(!region7.can_merge_with(&region8));

    // Test cannot merge vertical regions with different column spans
    let mut region9 = BackgroundRegion::new(0, 0, color1);
    region9.end_col = 5;
    let mut region10 = BackgroundRegion::new(1, 0, color1);
    region10.end_col = 6;
    assert!(!region9.can_merge_with(&region10));
}

#[test]
fn test_background_region_merge() {
    let color = Hsla::red();

    // Test horizontal merge
    let mut region1 = BackgroundRegion::new(0, 0, color);
    region1.end_col = 5;
    let mut region2 = BackgroundRegion::new(0, 6, color);
    region2.end_col = 10;
    region1.merge_with(&region2);
    assert_eq!(region1.start_col, 0);
    assert_eq!(region1.end_col, 10);
    assert_eq!(region1.start_line, 0);
    assert_eq!(region1.end_line, 0);

    // Test vertical merge
    let mut region3 = BackgroundRegion::new(0, 0, color);
    region3.end_col = 5;
    let mut region4 = BackgroundRegion::new(1, 0, color);
    region4.end_col = 5;
    region3.merge_with(&region4);
    assert_eq!(region3.start_col, 0);
    assert_eq!(region3.end_col, 5);
    assert_eq!(region3.start_line, 0);
    assert_eq!(region3.end_line, 1);
}

#[test]
fn test_merge_background_regions() {
    let color = Hsla::red();

    // Test merging multiple adjacent regions
    let regions = vec![
        BackgroundRegion::new(0, 0, color),
        BackgroundRegion::new(0, 1, color),
        BackgroundRegion::new(0, 2, color),
        BackgroundRegion::new(1, 0, color),
        BackgroundRegion::new(1, 1, color),
        BackgroundRegion::new(1, 2, color),
    ];

    let merged = merge_background_regions(regions);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].start_line, 0);
    assert_eq!(merged[0].end_line, 1);
    assert_eq!(merged[0].start_col, 0);
    assert_eq!(merged[0].end_col, 2);

    // Test with non-mergeable regions
    let color2 = Hsla::blue();
    let regions2 = vec![
        BackgroundRegion::new(0, 0, color),
        BackgroundRegion::new(0, 2, color),  // Gap at column 1
        BackgroundRegion::new(1, 0, color2), // Different color
    ];

    let merged2 = merge_background_regions(regions2);
    assert_eq!(merged2.len(), 3);
}
