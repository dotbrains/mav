use super::*;

impl FileFinderDelegate {
    pub(super) fn labels_for_match(
        &self,
        path_match: &Match,
        window: &mut Window,
        cx: &App,
    ) -> (HighlightedLabel, HighlightedLabel) {
        let path_style = self.project.read(cx).path_style(cx);
        let (file_name, file_name_positions, mut full_path, mut full_path_positions) =
            match &path_match {
                Match::History {
                    path: entry_path,
                    panel_match,
                } => {
                    let worktree_id = entry_path.project.worktree_id;
                    let worktree = self
                        .project
                        .read(cx)
                        .worktree_for_id(worktree_id, cx)
                        .filter(|worktree| worktree.read(cx).is_visible());

                    if let Some(panel_match) = panel_match {
                        self.labels_for_path_match(&panel_match.0, path_style)
                    } else if let Some(worktree) = worktree {
                        let worktree_store = self.project.read(cx).worktree_store();
                        let full_path = if should_hide_root_in_entry_path(&worktree_store, cx) {
                            entry_path.project.path.clone()
                        } else {
                            worktree.read(cx).root_name().join(&entry_path.project.path)
                        };
                        let mut components = full_path.components();
                        let filename = components.next_back().unwrap_or("");
                        let prefix = components.rest();
                        (
                            filename.to_string(),
                            Vec::new(),
                            prefix.display(path_style).to_string() + path_style.primary_separator(),
                            Vec::new(),
                        )
                    } else {
                        (
                            entry_path
                                .absolute
                                .file_name()
                                .map_or(String::new(), |f| f.to_string_lossy().into_owned()),
                            Vec::new(),
                            entry_path.absolute.parent().map_or(String::new(), |path| {
                                path.to_string_lossy().into_owned() + path_style.primary_separator()
                            }),
                            Vec::new(),
                        )
                    }
                }
                Match::Search(path_match) => self.labels_for_path_match(&path_match.0, path_style),
                Match::Channel {
                    channel_name,
                    string_match,
                    ..
                } => (
                    channel_name.to_string(),
                    string_match.positions.clone(),
                    "Channel Notes".to_string(),
                    vec![],
                ),
                Match::CreateNew(project_path) => (
                    format!("Create File: {}", project_path.path.display(path_style)),
                    vec![],
                    String::from(""),
                    vec![],
                ),
            };

        if file_name_positions.is_empty() {
            let user_home_path = util::paths::home_dir().to_string_lossy();
            if !user_home_path.is_empty() && full_path.starts_with(&*user_home_path) {
                full_path.replace_range(0..user_home_path.len(), "~");
                full_path_positions.retain_mut(|pos| {
                    if *pos >= user_home_path.len() {
                        *pos -= user_home_path.len();
                        *pos += 1;
                        true
                    } else {
                        false
                    }
                })
            }
        }

        if full_path.is_ascii() {
            let file_finder_settings = FileFinderSettings::get_global(cx);
            let max_width =
                FileFinder::modal_max_width(file_finder_settings.modal_max_width, window);
            let (normal_em, small_em) = {
                let style = window.text_style();
                let font_id = window.text_system().resolve_font(&style.font());
                let font_size = TextSize::Default.rems(cx).to_pixels(window.rem_size());
                let normal = cx
                    .text_system()
                    .em_width(font_id, font_size)
                    .unwrap_or(px(16.));
                let font_size = TextSize::Small.rems(cx).to_pixels(window.rem_size());
                let small = cx
                    .text_system()
                    .em_width(font_id, font_size)
                    .unwrap_or(px(10.));
                (normal, small)
            };
            let budget = full_path_budget(&file_name, normal_em, small_em, max_width);
            // If the computed budget is zero, we certainly won't be able to achieve it,
            // so no point trying to elide the path.
            if budget > 0 && full_path.len() > budget {
                let components = PathComponentSlice::new(&full_path);
                if let Some(elided_range) =
                    components.elision_range(budget - 1, &full_path_positions)
                {
                    let elided_len = elided_range.end - elided_range.start;
                    let placeholder = "…";
                    full_path_positions.retain_mut(|mat| {
                        if *mat >= elided_range.end {
                            *mat -= elided_len;
                            *mat += placeholder.len();
                        } else if *mat >= elided_range.start {
                            return false;
                        }
                        true
                    });
                    full_path.replace_range(elided_range, placeholder);
                }
            }
        }

        (
            HighlightedLabel::new(file_name, file_name_positions),
            HighlightedLabel::new(full_path, full_path_positions)
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
    }

    pub(super) fn labels_for_path_match(
        &self,
        path_match: &PathMatch,
        path_style: PathStyle,
    ) -> (String, Vec<usize>, String, Vec<usize>) {
        let full_path = path_match.path_prefix.join(&path_match.path);
        let mut path_positions = path_match.positions.clone();

        let file_name = full_path.file_name().unwrap_or("");
        let file_name_start = full_path.as_unix_str().len() - file_name.len();
        let file_name_positions = path_positions
            .iter()
            .filter_map(|pos| {
                if pos >= &file_name_start {
                    Some(pos - file_name_start)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let full_path = full_path
            .display(path_style)
            .trim_end_matches(&file_name)
            .to_string();
        path_positions.retain(|idx| *idx < full_path.len());

        debug_assert!(
            file_name_positions
                .iter()
                .all(|ix| file_name[*ix..].chars().next().is_some()),
            "invalid file name positions {file_name:?} {file_name_positions:?}"
        );
        debug_assert!(
            path_positions
                .iter()
                .all(|ix| full_path[*ix..].chars().next().is_some()),
            "invalid path positions {full_path:?} {path_positions:?}"
        );

        (
            file_name.to_string(),
            file_name_positions,
            full_path,
            path_positions,
        )
    }

    /// Attempts to resolve an absolute file path and update the search matches if found.
    ///
    /// If the query path resolves to an absolute file that exists in the project,
    /// this method will find the corresponding worktree and relative path, create a
    /// match for it, and update the picker's search results.
    ///
    /// Returns `true` if the absolute path exists, otherwise returns `false`.
    pub(super) fn lookup_absolute_path(
        &self,
        query: FileSearchQuery,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<bool> {
        cx.spawn_in(window, async move |picker, cx| {
            let Some(project) = picker
                .read_with(cx, |picker, _| picker.delegate.project.clone())
                .log_err()
            else {
                return false;
            };

            let query_path = Path::new(query.path_query());
            let mut path_matches = Vec::new();

            let abs_file_exists = project
                .update(cx, |this, cx| {
                    this.resolve_abs_file_path(query.path_query(), cx)
                })
                .await
                .is_some();

            if abs_file_exists {
                project.update(cx, |project, cx| {
                    if let Some((worktree, relative_path)) = project.find_worktree(query_path, cx) {
                        path_matches.push(ProjectPanelOrdMatch(PathMatch {
                            score: 1.0,
                            positions: Vec::new(),
                            worktree_id: worktree.read(cx).id().to_usize(),
                            path: relative_path,
                            path_prefix: RelPath::empty_arc(),
                            is_dir: false, // File finder doesn't support directories
                            distance_to_relative_ancestor: usize::MAX,
                        }));
                    }
                });
            }

            picker
                .update_in(cx, |picker, _, cx| {
                    let picker_delegate = &mut picker.delegate;
                    let search_id = util::post_inc(&mut picker_delegate.search_count);
                    picker_delegate.set_search_matches(search_id, false, query, path_matches, cx);

                    anyhow::Ok(())
                })
                .log_err();
            abs_file_exists
        })
    }
}
