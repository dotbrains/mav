use super::*;

pub(super) struct BookmarkTestContext {
    pub(super) project: Entity<Project>,
    pub(super) editor: Entity<Editor>,
    pub(super) cx: VisualTestContext,
}

impl BookmarkTestContext {
    pub(super) async fn new(sample_text: &str, cx: &mut TestAppContext) -> BookmarkTestContext {
        init_test(cx, |_| {});

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/a"),
            json!({
                "main.rs": sample_text,
            }),
        )
        .await;
        let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let mut visual_cx = VisualTestContext::from_window(*window, cx);
        let worktree_id = workspace.update_in(&mut visual_cx, |workspace, _window, cx| {
            workspace.project().update(cx, |project, cx| {
                project.worktrees(cx).next().unwrap().read(cx).id()
            })
        });

        let buffer = project
            .update(&mut visual_cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path("main.rs")), cx)
            })
            .await
            .unwrap();

        let (editor, editor_cx) = cx.add_window_view(|window, cx| {
            Editor::new(
                EditorMode::full(),
                MultiBuffer::build_from_buffer(buffer, cx),
                Some(project.clone()),
                window,
                cx,
            )
        });
        let cx = editor_cx.clone();

        BookmarkTestContext {
            project,
            editor,
            cx,
        }
    }

    pub(super) fn abs_path(&self) -> Arc<Path> {
        let project_path = self.editor.read_with(&self.cx, |editor, cx| {
            editor.active_project_path(cx).unwrap()
        });
        self.project.read_with(&self.cx, |project, cx| {
            project
                .absolute_path(&project_path, cx)
                .map(Arc::from)
                .unwrap()
        })
    }

    pub(super) fn all_bookmarks(&self) -> BTreeMap<Arc<Path>, Vec<SerializedBookmark>> {
        self.project.read_with(&self.cx, |project, cx| {
            project
                .bookmark_store()
                .read(cx)
                .all_serialized_bookmarks(cx)
        })
    }

    pub(super) fn assert_bookmarked_file_count(&self, expected_count: usize) {
        assert_eq!(expected_count, self.all_bookmarks().len());
    }

    pub(super) fn assert_bookmark_rows(&self, expected_rows: Vec<u32>) {
        let abs_path = self.abs_path();
        let bookmarks = self.all_bookmarks();
        if expected_rows.is_empty() {
            assert!(
                !bookmarks.contains_key(&abs_path),
                "Expected no bookmarks for {}",
                abs_path.display()
            );
        } else {
            let mut rows: Vec<u32> = bookmarks
                .get(&abs_path)
                .unwrap()
                .iter()
                .map(|b| b.row)
                .collect();
            rows.sort();
            assert_eq!(expected_rows, rows);
        }
    }

    pub(super) fn assert_bookmark_labels(&self, expected_labels: Vec<(u32, &str)>) {
        let abs_path = self.abs_path();
        let bookmarks = self.all_bookmarks();
        let mut labels: Vec<(u32, &str)> = bookmarks
            .get(&abs_path)
            .unwrap()
            .iter()
            .map(|bookmark| (bookmark.row, bookmark.label.as_str()))
            .collect();
        labels.sort_by_key(|(row, _)| *row);
        assert_eq!(expected_labels, labels);
    }

    pub(super) fn confirm_action_available(&mut self) -> bool {
        self.cx
            .update(|window, cx| window.is_action_available(&menu::Confirm, cx))
    }

    pub(super) fn select_rows(&mut self, rows: &[u32]) {
        assert!(!rows.is_empty(), "expected at least one row to select");

        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(
                        rows.iter()
                            .copied()
                            .map(|row| Point::new(row, 0)..Point::new(row, 0)),
                    )
                });
            });
    }

    pub(super) fn prompt_blocks(&mut self) -> Vec<(DisplayRow, u32)> {
        self.editor.update(&mut self.cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let max_row = snapshot.max_point().row().next_row();

            snapshot
                .blocks_in_range(DisplayRow(0)..max_row)
                .filter_map(|(row, block)| match block {
                    crate::display_map::Block::Custom(_) => Some((row, block.height())),
                    _ => None,
                })
                .collect()
        })
    }

    pub(super) fn assert_prompt_block_count(&mut self, expected_count: usize) {
        assert_eq!(expected_count, self.prompt_blocks().len());
    }

    pub(super) fn draw_window(&mut self) {
        self.cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    pub(super) fn focus_bookmark_prompt_block(&mut self, block_index: usize) {
        self.draw_window();

        let prompt_blocks = self.prompt_blocks();
        let (block_row, block_height) = *prompt_blocks
            .get(block_index)
            .expect("expected bookmark prompt block");

        let click_position =
            self.editor
                .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                    let snapshot = editor.snapshot(window, cx);
                    let block_top = DisplayPoint::new(block_row, 0);
                    let relative_block_top = editor
                        .display_to_pixel_point(block_top, &snapshot, window, cx)
                        .expect("expected prompt block to be visible");
                    let line_height = editor
                        .style(cx)
                        .text
                        .line_height_in_pixels(window.rem_size());
                    let editor_origin = editor
                        .last_position_map
                        .as_ref()
                        .expect("expected editor position map")
                        .text_hitbox
                        .bounds
                        .origin;
                    let editor_center_x = editor
                        .last_bounds
                        .expect("expected editor bounds")
                        .center()
                        .x;

                    gpui::Point {
                        x: editor_center_x,
                        y: editor_origin.y
                            + relative_block_top.y
                            + line_height * (block_height as f32 / 2.),
                    }
                });

        self.cx
            .simulate_click(click_position, gpui::Modifiers::none());
        self.cx.run_until_parked();

        assert!(
            self.confirm_action_available(),
            "expected bookmark prompt block to be focused"
        );
    }

    pub(super) fn confirm_bookmark_prompt_at_block_index(
        &mut self,
        block_index: usize,
        label: &str,
    ) {
        // Confirming a PromptEditor returns focus to the parent editor, so each remaining
        // prompt block must be focused explicitly before typing into it.
        self.focus_bookmark_prompt_block(block_index);
        self.confirm_bookmark_prompt(label);
    }

    pub(super) fn cursor_row(&mut self) -> u32 {
        self.cursor_point().row
    }

    pub(super) fn cursor_point(&mut self) -> Point {
        self.editor.update(&mut self.cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            editor.selections.newest::<Point>(&snapshot).head()
        })
    }

    pub(super) fn move_to_row(&mut self, row: u32) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.move_to_beginning(&MoveToBeginning, window, cx);
                for _ in 0..row {
                    editor.move_down(&MoveDown, window, cx);
                }
            });
    }

    pub(super) fn toggle_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.toggle_bookmark(&actions::ToggleBookmark, window, cx);
            });
    }

    pub(super) fn confirm_bookmark_prompt(&mut self, label: &str) {
        if !label.is_empty() {
            self.cx.simulate_input(label);
        }
        self.cx.dispatch_action(menu::Confirm);
        self.cx.run_until_parked();
    }

    pub(super) fn add_bookmark_with_label(&mut self, label: &str) {
        self.toggle_bookmark();
        self.confirm_bookmark_prompt(label);
    }

    pub(super) fn toggle_bookmarks_at_rows(&mut self, rows: &[u32]) {
        for &row in rows {
            self.move_to_row(row);
            self.add_bookmark_with_label("");
        }
    }

    pub(super) fn go_to_next_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.go_to_next_bookmark(&actions::GoToNextBookmark, window, cx);
            });
    }

    pub(super) fn go_to_previous_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.go_to_previous_bookmark(&actions::GoToPreviousBookmark, window, cx);
            });
    }
}
