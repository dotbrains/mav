use super::*;

impl Editor {
    pub(super) fn find_syntax_node_boundary(
        &self,
        selection_pos: MultiBufferOffset,
        move_to_end: bool,
        display_map: &DisplaySnapshot,
        buffer: &MultiBufferSnapshot,
    ) -> MultiBufferOffset {
        let old_range = selection_pos..selection_pos;
        let mut new_pos = selection_pos;
        let mut search_range = old_range;
        while let Some((node, range)) = buffer.syntax_ancestor(search_range.clone()) {
            search_range = range.clone();
            if !node.is_named()
                || display_map.intersects_fold(range.start)
                || display_map.intersects_fold(range.end)
                // If cursor is already at the end of the syntax node, continue searching
                || (move_to_end && range.end == selection_pos)
                // If cursor is already at the start of the syntax node, continue searching
                || (!move_to_end && range.start == selection_pos)
            {
                continue;
            }

            // If we found a string_content node, find the largest parent that is still string_content
            // Enables us to skip to the end of strings without taking multiple steps inside the string
            let (_, final_range) = if node.kind() == "string_content" {
                let mut current_node = node;
                let mut current_range = range;
                while let Some((parent, parent_range)) =
                    buffer.syntax_ancestor(current_range.clone())
                {
                    if parent.kind() == "string_content" {
                        current_node = parent;
                        current_range = parent_range;
                    } else {
                        break;
                    }
                }

                (current_node, current_range)
            } else {
                (node, range)
            };

            new_pos = if move_to_end {
                final_range.end
            } else {
                final_range.start
            };

            break;
        }

        new_pos
    }

    pub(super) fn move_cursors_to_syntax_nodes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        move_to_end: bool,
    ) {
        let old_selections: Box<[_]> = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into();
        if old_selections.is_empty() {
            return;
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = self.buffer.read(cx).snapshot(cx);

        let new_selections = old_selections
            .iter()
            .map(|selection| {
                if !selection.is_empty() {
                    return selection.clone();
                }

                let selection_pos = selection.head();
                let new_pos = self.find_syntax_node_boundary(
                    selection_pos,
                    move_to_end,
                    &display_map,
                    &buffer,
                );

                Selection {
                    id: selection.id,
                    start: new_pos,
                    end: new_pos,
                    goal: SelectionGoal::None,
                    reversed: false,
                }
            })
            .collect::<Vec<_>>();

        self.change_selections(Default::default(), window, cx, |s| {
            s.select(new_selections);
        });
        self.request_autoscroll(Autoscroll::newest(), cx);
    }

    pub(super) fn select_to_syntax_nodes(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        move_to_end: bool,
    ) {
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = self.buffer.read(cx).snapshot(cx);
        let old_selections = self.selections.all::<MultiBufferOffset>(&display_map);

        let new_selections = old_selections
            .iter()
            .map(|selection| {
                let new_pos = self.find_syntax_node_boundary(
                    selection.head(),
                    move_to_end,
                    &display_map,
                    &buffer,
                );

                let mut new_selection = selection.clone();
                new_selection.set_head(new_pos, SelectionGoal::None);
                new_selection
            })
            .collect::<Vec<_>>();

        self.change_selections(Default::default(), window, cx, |s| {
            s.select(new_selections);
        });
    }
}
