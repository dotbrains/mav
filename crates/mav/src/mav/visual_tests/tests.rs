use super::*;

fn create_test_image(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
    let mut img = ImageBuffer::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = color;
    }
    img
}

#[test]
fn test_identical_images_match() {
    let img1 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));
    let img2 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));

    let comparison = compare_screenshots(&img1, &img2, 0);

    assert_eq!(comparison.match_percentage, 1.0);
    assert_eq!(comparison.diff_pixel_count, 0);
    assert!(comparison.matches(0.0));
}

#[test]
fn test_different_images_dont_match() {
    let img1 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));
    let img2 = create_test_image(100, 100, Rgba([0, 255, 0, 255]));

    let comparison = compare_screenshots(&img1, &img2, 0);

    assert_eq!(comparison.match_percentage, 0.0);
    assert_eq!(comparison.diff_pixel_count, 10000);
    assert!(!comparison.matches(0.5));
}

#[test]
fn test_similar_images_match_with_threshold() {
    let img1 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));
    let img2 = create_test_image(100, 100, Rgba([250, 5, 0, 255]));

    let comparison_strict = compare_screenshots(&img1, &img2, 0);
    assert_eq!(comparison_strict.match_percentage, 0.0);

    let comparison_lenient = compare_screenshots(&img1, &img2, 10);
    assert_eq!(comparison_lenient.match_percentage, 1.0);
}

#[test]
fn test_different_size_images() {
    let img1 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));
    let img2 = create_test_image(200, 200, Rgba([255, 0, 0, 255]));

    let comparison = compare_screenshots(&img1, &img2, 0);

    assert_eq!(comparison.match_percentage, 0.0);
    assert!(comparison.diff_image.is_none());
}

#[test]
fn test_partial_difference() {
    let mut img1 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));
    let img2 = create_test_image(100, 100, Rgba([255, 0, 0, 255]));

    for x in 0..50 {
        for y in 0..100 {
            img1.put_pixel(x, y, Rgba([0, 255, 0, 255]));
        }
    }

    let comparison = compare_screenshots(&img1, &img2, 0);

    assert_eq!(comparison.match_percentage, 0.5);
    assert_eq!(comparison.diff_pixel_count, 5000);
    assert!(comparison.matches(0.5));
    assert!(!comparison.matches(0.49));
}

#[test]
#[ignore]
fn test_visual_test_smoke() {
    let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));

    let _window = cx
        .open_offscreen_window_default(|_, cx| cx.new(|_| Empty))
        .expect("Failed to open offscreen window");

    cx.run_until_parked();
}

#[test]
#[ignore]
fn test_workspace_opens() {
    let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
    let app_state = init_visual_test(&mut cx);

    gpui::block_on(async {
        app_state
            .fs
            .as_fake()
            .insert_tree(
                "/project",
                serde_json::json!({
                    "src": {
                        "main.rs": "fn main() {\n    println!(\"Hello, world!\");\n}\n"
                    }
                }),
            )
            .await;
    });

    let workspace_result = gpui::block_on(open_test_workspace(app_state, &mut cx));
    assert!(
        workspace_result.is_ok(),
        "Failed to open workspace: {:?}",
        workspace_result.err()
    );

    cx.run_until_parked();
}

/// This test captures a screenshot of an empty Mav workspace.
///
/// Note: This test is ignored by default because:
/// 1. It requires macOS with Screen Recording permission granted
/// 2. It must run on the main thread (standard test threads won't work)
/// 3. Screenshot capture may fail in CI environments without display access
///
/// The test will gracefully handle screenshot failures and print an error
/// message rather than failing hard, to allow running in environments
/// where screen capture isn't available.
#[test]
#[ignore]
fn test_workspace_screenshot() {
    let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
    let app_state = init_visual_test(&mut cx);

    gpui::block_on(async {
        app_state
            .fs
            .as_fake()
            .insert_tree(
                "/project",
                serde_json::json!({
                    "src": {
                        "main.rs": "fn main() {\n    println!(\"Hello, world!\");\n}\n"
                    },
                    "README.md": "# Test Project\n\nThis is a test project for visual testing.\n"
                }),
            )
            .await;
    });

    let workspace =
        gpui::block_on(open_test_workspace(app_state, &mut cx)).expect("Failed to open workspace");

    gpui::block_on(async {
        wait_for_ui_stabilization(&cx).await;

        let screenshot_result = cx.capture_screenshot(workspace.into());

        match screenshot_result {
            Ok(screenshot) => {
                println!(
                    "Screenshot captured successfully: {}x{}",
                    screenshot.width(),
                    screenshot.height()
                );

                let output_dir = std::env::var("VISUAL_TEST_OUTPUT_DIR")
                    .unwrap_or_else(|_| "target/visual_tests".to_string());
                let output_path = Path::new(&output_dir).join("workspace_screenshot.png");

                if let Err(e) = std::fs::create_dir_all(&output_dir) {
                    eprintln!("Warning: Failed to create output directory: {}", e);
                }

                if let Err(e) = screenshot.save(&output_path) {
                    eprintln!("Warning: Failed to save screenshot: {}", e);
                } else {
                    println!("Screenshot saved to: {}", output_path.display());
                }

                assert!(
                    screenshot.width() > 0,
                    "Screenshot width should be positive"
                );
                assert!(
                    screenshot.height() > 0,
                    "Screenshot height should be positive"
                );
            }
            Err(e) => {
                eprintln!(
                    "Screenshot capture failed (this may be expected in CI without screen recording permission): {}",
                    e
                );
            }
        }
    });

    cx.run_until_parked();
}
