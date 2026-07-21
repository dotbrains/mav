use super::*;

#[gpui::test]
async fn test_outline_keyboard_expand_collapse(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/test",
        json!({
            "src": {
                "lib.rs": indoc!("
                        mod outer {
                            pub struct OuterStruct {
                                field: String,
                            }
                            impl OuterStruct {
                                pub fn new() -> Self {
                                    Self { field: String::new() }
                                }
                                pub fn method(&self) {
                                    println!(\"{}\", self.field);
                                }
                            }
                            mod inner {
                                pub fn inner_function() {
                                    let x = 42;
                                    println!(\"{}\", x);
                                }
                                pub struct InnerStruct {
                                    value: i32,
                                }
                            }
                        }
                        fn main() {
                            let s = outer::OuterStruct::new();
                            s.method();
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

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/test/src/lib.rs"),
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
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
    cx.run_until_parked();

    // Force another update cycle to ensure outlines are fetched
    outline_panel.update_in(cx, |panel, window, cx| {
        panel.update_non_fs_items(window, cx);
        panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(500));
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
outline: mod outer  <==== selected
  outline: pub struct OuterStruct
outline: field
  outline: impl OuterStruct
outline: pub fn new
outline: pub fn method
  outline: mod inner
outline: pub fn inner_function
outline: pub struct InnerStruct
  outline: value
outline: fn main"
            )
        );
    });

    let parent_outline = outline_panel
        .read_with(cx, |panel, _cx| {
            panel
                .cached_entries
                .iter()
                .find_map(|entry| match &entry.entry {
                    PanelEntry::Outline(OutlineEntry::Outline(outline))
                        if panel
                            .outline_children_cache
                            .get(&outline.range.start.buffer_id)
                            .and_then(|children_map| {
                                let key = (outline.range.clone(), outline.depth);
                                children_map.get(&key)
                            })
                            .copied()
                            .unwrap_or(false) =>
                    {
                        Some(entry.entry.clone())
                    }
                    _ => None,
                })
        })
        .expect("Should find an outline with children");

    outline_panel.update_in(cx, |panel, window, cx| {
        panel.select_entry(parent_outline.clone(), true, window, cx);
        panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
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
outline: mod outer  <==== selected
outline: fn main"
            )
        );
    });

    outline_panel.update_in(cx, |panel, window, cx| {
        panel.expand_selected_entry(&ExpandSelectedEntry, window, cx);
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
outline: mod outer  <==== selected
  outline: pub struct OuterStruct
outline: field
  outline: impl OuterStruct
outline: pub fn new
outline: pub fn method
  outline: mod inner
outline: pub fn inner_function
outline: pub struct InnerStruct
  outline: value
outline: fn main"
            )
        );
    });

    outline_panel.update_in(cx, |panel, window, cx| {
        panel.collapsed_entries.clear();
        panel.update_cached_entries(None, window, cx);
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update_in(cx, |panel, window, cx| {
        let outlines_with_children: Vec<_> = panel
            .cached_entries
            .iter()
            .filter_map(|entry| match &entry.entry {
                PanelEntry::Outline(OutlineEntry::Outline(outline))
                    if panel
                        .outline_children_cache
                        .get(&outline.range.start.buffer_id)
                        .and_then(|children_map| {
                            let key = (outline.range.clone(), outline.depth);
                            children_map.get(&key)
                        })
                        .copied()
                        .unwrap_or(false) =>
                {
                    Some(entry.entry.clone())
                }
                _ => None,
            })
            .collect();

        for outline in outlines_with_children {
            panel.select_entry(outline, false, window, cx);
            panel.collapse_selected_entry(&CollapseSelectedEntry, window, cx);
        }
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
outline: mod outer
outline: fn main"
            )
        );
    });

    let collapsed_entries_count =
        outline_panel.read_with(cx, |panel, _| panel.collapsed_entries.len());
    assert!(
        collapsed_entries_count > 0,
        "Should have collapsed entries tracked"
    );
}
