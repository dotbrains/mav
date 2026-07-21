use super::*;

#[gpui::test]
async fn test_navigating_in_singleton(cx: &mut TestAppContext) {
    init_test(cx);

    let root = path!("/root");
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        root,
        json!({
            "src": {
                "lib.rs": indoc!("
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OutlineEntryExcerpt {
id: ExcerptId,
buffer_id: BufferId,
range: ExcerptRange<language::Anchor>,
}"),
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
    project.read_with(cx, |project, _| project.languages().add(rust_lang()));
    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);
    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.set_active(true, window, cx)
        });
    });

    let _editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/src/lib.rs")),
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .await
        .expect("Failed to open Rust source file")
        .downcast::<Editor>()
        .expect("Should open an editor for Rust source file");

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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
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
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id  <==== selected
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id  <==== selected
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_next(&SelectNext, window, cx);
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
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id  <==== selected
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id  <==== selected
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
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
outline: struct OutlineEntryExcerpt  <==== selected
  outline: id
  outline: buffer_id
  outline: range"
            )
        );
    });

    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.select_previous(&SelectPrevious, window, cx);
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
outline: struct OutlineEntryExcerpt
  outline: id
  outline: buffer_id
  outline: range  <==== selected"
            )
        );
    });
}
