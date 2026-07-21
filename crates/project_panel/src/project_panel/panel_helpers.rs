use super::*;

impl ProjectPanel {
    pub(super) fn panel_scrollbars(
        &self,
        horizontal_scroll: bool,
        cx: &mut Context<Self>,
    ) -> Scrollbars<UniformListScrollHandle> {
        let mut scrollbars = Scrollbars::for_settings::<ProjectPanelScrollbarProxy>()
            .tracked_scroll_handle(&self.scroll_handle);
        if horizontal_scroll {
            scrollbars = scrollbars.with_track_along(
                ScrollAxes::Horizontal,
                cx.theme().colors().editor_background,
            );
        }
        scrollbars.notify_content()
    }

    pub(super) fn render_context_menu_layer(
        menu: Entity<ContextMenu>,
        position: Point<Pixels>,
    ) -> AnyElement {
        deferred(
            anchored()
                .position(position)
                .anchor(gpui::Anchor::TopLeft)
                .child(menu),
        )
        .with_priority(3)
        .into_any_element()
    }

    pub(super) fn visible_entry_count(&self) -> usize {
        self.state
            .visible_entries
            .iter()
            .map(|worktree| worktree.entries.len())
            .sum()
    }

    pub(super) fn should_show_sticky_entries(&self, panel_settings: &ProjectPanelSettings) -> bool {
        if panel_settings.sticky_scroll {
            let is_scrollable = self.scroll_handle.is_scrollable();
            let is_scrolled = self.scroll_handle.offset().y < px(0.);
            is_scrollable && is_scrolled
        } else {
            false
        }
    }

    pub(super) fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("ProjectPanel");
        dispatch_context.add("menu");

        let identifier = if self.filename_editor.focus_handle(cx).is_focused(window) {
            "editing"
        } else {
            "not_editing"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }
    pub(super) fn reveal_entry(
        &mut self,
        project: Entity<Project>,
        entry_id: ProjectEntryId,
        skip_ignored: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let worktree = project
            .read(cx)
            .worktree_for_entry(entry_id, cx)
            .context("can't reveal a non-existent entry in the project panel")?;
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        let is_ignored = worktree
            .entry_for_id(entry_id)
            .is_none_or(|entry| entry.is_ignored && !entry.is_always_included);
        if skip_ignored && is_ignored {
            if self.index_for_entry(entry_id, worktree_id).is_none() {
                anyhow::bail!("can't reveal an ignored entry in the project panel");
            }

            self.selection = Some(SelectedEntry {
                worktree_id,
                entry_id,
            });
            self.marked_entries.clear();
            self.marked_entries.push(SelectedEntry {
                worktree_id,
                entry_id,
            });
            self.autoscroll(cx);
            cx.notify();
            return Ok(());
        }
        let is_active_item_file_diff_view = self
            .workspace
            .upgrade()
            .and_then(|ws| ws.read(cx).active_item(cx))
            .map(|item| item.act_as_type(TypeId::of::<FileDiffView>(), cx).is_some())
            .unwrap_or(false);
        if is_active_item_file_diff_view {
            return Ok(());
        }

        self.expand_entry(worktree_id, entry_id, cx);
        self.update_visible_entries(Some((worktree_id, entry_id)), false, true, window, cx);
        self.marked_entries.clear();
        self.marked_entries.push(SelectedEntry {
            worktree_id,
            entry_id,
        });
        cx.notify();
        Ok(())
    }

    pub(super) fn find_active_indent_guide(
        &self,
        indent_guides: &[IndentGuideLayout],
        cx: &App,
    ) -> Option<usize> {
        let (worktree, entry) = self.selected_entry(cx)?;

        // Find the parent entry of the indent guide, this will either be the
        // expanded folder we have selected, or the parent of the currently
        // selected file/collapsed directory
        let mut entry = entry;
        loop {
            let is_expanded_dir = entry.is_dir()
                && self
                    .state
                    .expanded_dir_ids
                    .get(&worktree.id())
                    .map(|ids| ids.binary_search(&entry.id).is_ok())
                    .unwrap_or(false);
            if is_expanded_dir {
                break;
            }
            entry = worktree.entry_for_path(&entry.path.parent()?)?;
        }

        let (active_indent_range, depth) = {
            let (worktree_ix, child_offset, ix) = self.index_for_entry(entry.id, worktree.id())?;
            let child_paths = &self.state.visible_entries[worktree_ix].entries;
            let mut child_count = 0;
            let depth = entry.path.ancestors().count();
            while let Some(entry) = child_paths.get(child_offset + child_count + 1) {
                if entry.path.ancestors().count() <= depth {
                    break;
                }
                child_count += 1;
            }

            let start = ix + 1;
            let end = start + child_count;

            let visible_worktree = &self.state.visible_entries[worktree_ix];
            let visible_worktree_entries = visible_worktree.index.get_or_init(|| {
                visible_worktree
                    .entries
                    .iter()
                    .map(|e| e.path.clone())
                    .collect()
            });

            // Calculate the actual depth of the entry, taking into account that directories can be auto-folded.
            let (depth, _) = Self::calculate_depth_and_difference(entry, visible_worktree_entries);
            (start..end, depth)
        };

        let candidates = indent_guides
            .iter()
            .enumerate()
            .filter(|(_, indent_guide)| indent_guide.offset.x == depth);

        for (i, indent) in candidates {
            // Find matches that are either an exact match, partially on screen, or inside the enclosing indent
            if active_indent_range.start <= indent.offset.y + indent.length
                && indent.offset.y <= active_indent_range.end
            {
                return Some(i);
            }
        }
        None
    }
}
