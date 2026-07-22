use super::*;

/// Visual test for the Tool Permissions Settings UI page
///
/// Takes a screenshot showing the tool config page with matched patterns and verdict.
#[cfg(target_os = "macos")]
fn run_tool_permissions_visual_tests(
    app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    _update_baseline: bool,
) -> Result<TestResult> {
    use agent_settings::{AgentSettings, CompiledRegex, ToolPermissions, ToolRules};
    use collections::HashMap;
    use mav_actions::OpenSettingsAt;
    use settings::ToolPermissionMode;

    // Set up tool permissions with "hi" as both always_deny and always_allow for terminal
    cx.update(|cx| {
        let mut tools = HashMap::default();
        tools.insert(
            Arc::from("terminal"),
            ToolRules {
                default: None,
                always_allow: vec![CompiledRegex::new("hi", false).unwrap()],
                always_deny: vec![CompiledRegex::new("hi", false).unwrap()],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        let mut settings = AgentSettings::get_global(cx).clone();
        settings.tool_permissions = ToolPermissions {
            default: ToolPermissionMode::Confirm,
            tools,
        };
        AgentSettings::override_global(settings, cx);
    });

    // Create a minimal workspace to dispatch the settings action from
    let window_size = size(px(900.0), px(700.0));
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_size,
    };

    let project = cx.update(|cx| {
        project::Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags {
                init_worktree_trust: false,
                ..Default::default()
            },
            cx,
        )
    });

    let workspace_window: WindowHandle<MultiWorkspace> = cx
        .update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |window, cx| {
                    let workspace = cx.new(|cx| {
                        Workspace::new(None, project.clone(), app_state.clone(), window, cx)
                    });
                    cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
                },
            )
        })
        .context("Failed to open workspace window for settings test")?;

    cx.run_until_parked();

    // Dispatch the OpenSettingsAt action to open settings at the tool_permissions path
    workspace_window
        .update(cx, |_workspace, window, cx| {
            window.dispatch_action(
                Box::new(OpenSettingsAt {
                    path: "agent.tool_permissions".to_string(),
                    target: None,
                }),
                cx,
            );
        })
        .context("Failed to dispatch OpenSettingsAt action")?;

    cx.run_until_parked();

    // Give the settings window time to open and render
    for _ in 0..10 {
        cx.advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    // Find the settings window - it should be the newest window (last in the list)
    let all_windows = cx.update(|cx| cx.windows());
    let settings_window = all_windows.last().copied().context("No windows found")?;

    let output_dir = std::env::var("VISUAL_TEST_OUTPUT_DIR")
        .unwrap_or_else(|_| "target/visual_tests".to_string());
    std::fs::create_dir_all(&output_dir).log_err();

    // Navigate to the tool permissions sub-page using the public API
    let settings_window_handle = settings_window
        .downcast::<settings_ui::SettingsWindow>()
        .context("Failed to downcast to SettingsWindow")?;

    settings_window_handle
        .update(cx, |settings_window, window, cx| {
            settings_window.navigate_to_sub_page("agent.tool_permissions", window, cx);
        })
        .context("Failed to navigate to tool permissions sub-page")?;

    cx.run_until_parked();

    // Give the sub-page time to render
    for _ in 0..10 {
        cx.advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    // Now navigate into a specific tool (Terminal) to show the tool config page
    settings_window_handle
        .update(cx, |settings_window, window, cx| {
            settings_window.push_dynamic_sub_page(
                "Terminal",
                "Configure Tool Rules",
                None,
                true,
                settings_ui::pages::render_terminal_tool_config,
                window,
                cx,
            );
        })
        .context("Failed to navigate to Terminal tool config")?;

    cx.run_until_parked();

    // Give the tool config page time to render
    for _ in 0..10 {
        cx.advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    // Refresh and redraw so the "Test Your Rules" input is present
    cx.update_window(settings_window, |_, window, cx| {
        window.draw(cx).clear();
    })
    .log_err();
    cx.run_until_parked();

    cx.update_window(settings_window, |_, window, _cx| {
        window.refresh();
    })
    .log_err();
    cx.run_until_parked();

    // Focus the first tab stop in the window (the "Test Your Rules" editor
    // has tab_index(0) and tab_stop(true)) and type "hi" into it.
    cx.update_window(settings_window, |_, window, cx| {
        window.focus_next(cx);
    })
    .log_err();
    cx.run_until_parked();

    cx.simulate_input(settings_window, "hi");

    // Let the UI update with the matched patterns
    for _ in 0..5 {
        cx.advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
    }

    // Refresh and redraw
    cx.update_window(settings_window, |_, window, cx| {
        window.draw(cx).clear();
    })
    .log_err();
    cx.run_until_parked();

    cx.update_window(settings_window, |_, window, _cx| {
        window.refresh();
    })
    .log_err();
    cx.run_until_parked();

    // Save screenshot: Tool config page with "hi" typed and matched patterns visible
    let tool_config_output_path =
        PathBuf::from(&output_dir).join("tool_permissions_test_rules.png");

    if let Ok(screenshot) = cx.capture_screenshot(settings_window) {
        screenshot.save(&tool_config_output_path).log_err();
        println!(
            "Screenshot (test rules) saved to: {}",
            tool_config_output_path.display()
        );
    }

    // Clean up - close the settings window
    cx.update_window(settings_window, |_, window, _cx| {
        window.remove_window();
    })
    .log_err();

    // Close the workspace window
    cx.update_window(workspace_window.into(), |_, window, _cx| {
        window.remove_window();
    })
    .log_err();

    cx.run_until_parked();

    // Give background tasks time to finish
    for _ in 0..5 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    // Return success - we're just capturing screenshots, not comparing baselines
    Ok(TestResult::Passed)
}
