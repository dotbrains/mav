use super::*;

#[gpui::test]
async fn test_outline_click_toggle_behavior(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/test",
        json!({
            "src": {
                "main.rs": indoc!("
                        struct Config {
                            name: String,
                            value: i32,
                        }
                        impl Config {
                            fn new(name: String) -> Self {
                                Self { name, value: 0 }
                            }
                            fn get_value(&self) -> i32 {
                                self.value
                            }
                        }
                        enum Status {
                            Active,
                            Inactive,
                        }
                        fn process_config(config: Config) -> Status {
                            if config.get_value() > 0 {
                                Status::Active
                            } else {
                                Status::Inactive
                            }
                        }
                        fn main() {
                            let config = Config::new(\"test\".to_string());
                            let status = process_config(config);
                        }
                    "),
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
    project.read_with(cx, |project, _| project.languages().add(rust_lang()));

    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);

    outline_panel.update_in(cx, |outline_panel, window, cx| {
        outline_panel.set_active(true, window, cx)
    });

    let _editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/test/src/main.rs"),
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, _cx| {
        outline_panel.selected_entry = SelectedEntry::None;
    });

    // Check initial state - all entries should be expanded by default
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Config
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
            )
        );
    });

    outline_panel.update(cx, |outline_panel, _cx| {
        outline_panel.selected_entry = SelectedEntry::None;
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_first(&SelectFirst, window, cx);
        });
    });

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Config  <==== selected
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        });
    });

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Config  <==== selected
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
        });
    });

    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Config  <==== selected
  outline: name
  outline: value
outline: impl Config
  outline: fn new
  outline: fn get_value
outline: enum Status
  outline: Active
  outline: Inactive
outline: fn process_config
outline: fn main"
            )
        );
    });
}
