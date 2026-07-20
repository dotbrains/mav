use super::*;

impl Editor {
    pub fn transact(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) -> Option<TransactionId> {
        self.with_selection_effects_deferred(window, cx, |this, window, cx| {
            this.start_transaction_at(Instant::now(), window, cx);
            update(this, window, cx);
            this.end_transaction_at(Instant::now(), cx)
        })
    }

    pub fn start_transaction_at(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        self.end_selection(window, cx);
        if let Some(tx_id) = self
            .buffer
            .update(cx, |buffer, cx| buffer.start_transaction_at(now, cx))
        {
            self.selection_history
                .insert_transaction(tx_id, self.selections.disjoint_anchors_arc());
            cx.emit(EditorEvent::TransactionBegun {
                transaction_id: tx_id,
            });
            Some(tx_id)
        } else {
            None
        }
    }

    pub fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(transaction_id) = self
            .buffer
            .update(cx, |buffer, cx| buffer.end_transaction_at(now, cx))
        {
            if let Some((_, end_selections)) =
                self.selection_history.transaction_mut(transaction_id)
            {
                *end_selections = Some(self.selections.disjoint_anchors_arc());
            } else {
                log::error!("unexpectedly ended a transaction that wasn't started by this editor");
            }

            cx.emit(EditorEvent::Edited { transaction_id });
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn modify_transaction_selection_history(
        &mut self,
        transaction_id: TransactionId,
        modify: impl FnOnce(&mut (Arc<[Selection<Anchor>]>, Option<Arc<[Selection<Anchor>]>>)),
    ) -> bool {
        self.selection_history
            .transaction_mut(transaction_id)
            .map(modify)
            .is_some()
    }

    pub fn toggle_focus(
        workspace: &mut Workspace,
        _: &actions::ToggleFocus,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(item) = workspace.recent_active_item_by_type::<Self>(cx) else {
            return;
        };
        workspace.activate_item(&item, true, true, window, cx);
    }

    pub fn set_gutter_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered != self.gutter_hovered {
            self.gutter_hovered = hovered;
            cx.notify();
        }
    }

    pub fn insert_blocks(
        &mut self,
        blocks: impl IntoIterator<Item = BlockProperties<Anchor>>,
        autoscroll: Option<Autoscroll>,
        cx: &mut Context<Self>,
    ) -> Vec<CustomBlockId> {
        let blocks = self
            .display_map
            .update(cx, |display_map, cx| display_map.insert_blocks(blocks, cx));
        if let Some(autoscroll) = autoscroll {
            self.request_autoscroll(autoscroll, cx);
        }
        cx.notify();
        blocks
    }

    pub fn resize_blocks(
        &mut self,
        heights: HashMap<CustomBlockId, u32>,
        autoscroll: Option<Autoscroll>,
        cx: &mut Context<Self>,
    ) {
        self.display_map
            .update(cx, |display_map, cx| display_map.resize_blocks(heights, cx));
        if let Some(autoscroll) = autoscroll {
            self.request_autoscroll(autoscroll, cx);
        }
        cx.notify();
    }

    pub fn replace_blocks(
        &mut self,
        renderers: HashMap<CustomBlockId, RenderBlock>,
        autoscroll: Option<Autoscroll>,
        cx: &mut Context<Self>,
    ) {
        self.display_map
            .update(cx, |display_map, _cx| display_map.replace_blocks(renderers));
        if let Some(autoscroll) = autoscroll {
            self.request_autoscroll(autoscroll, cx);
        }
        cx.notify();
    }

    pub fn remove_blocks(
        &mut self,
        block_ids: HashSet<CustomBlockId>,
        autoscroll: Option<Autoscroll>,
        cx: &mut Context<Self>,
    ) {
        self.display_map.update(cx, |display_map, cx| {
            display_map.remove_blocks(block_ids, cx)
        });
        if let Some(autoscroll) = autoscroll {
            self.request_autoscroll(autoscroll, cx);
        }
        cx.notify();
    }

    pub fn row_for_block(
        &self,
        block_id: CustomBlockId,
        cx: &mut Context<Self>,
    ) -> Option<DisplayRow> {
        self.display_map
            .update(cx, |map, cx| map.row_for_block(block_id, cx))
    }

    pub(crate) fn set_focused_block(&mut self, focused_block: FocusedBlock) {
        self.focused_block = Some(focused_block);
    }

    pub(crate) fn take_focused_block(&mut self) -> Option<FocusedBlock> {
        self.focused_block.take()
    }

    pub fn longest_row(&self, cx: &mut App) -> DisplayRow {
        self.display_map
            .update(cx, |map, cx| map.snapshot(cx))
            .longest_row()
    }

    pub fn max_point(&self, cx: &mut App) -> DisplayPoint {
        self.display_map
            .update(cx, |map, cx| map.snapshot(cx))
            .max_point()
    }

    pub fn text(&self, cx: &App) -> String {
        self.buffer.read(cx).read(cx).text()
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.buffer.read(cx).read(cx).is_empty()
    }

    pub fn text_option(&self, cx: &App) -> Option<String> {
        let text = self.text(cx);
        let text = text.trim();

        if text.is_empty() {
            return None;
        }

        Some(text.to_string())
    }

    pub fn set_text(
        &mut self,
        text: impl Into<Arc<str>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transact(window, cx, |this, _, cx| {
            this.buffer
                .read(cx)
                .as_singleton()
                .expect("you can only call set_text on editors for singleton buffers")
                .update(cx, |buffer, cx| buffer.set_text(text, cx));
        });
    }

    pub fn display_text(&self, cx: &mut App) -> String {
        self.display_map
            .update(cx, |map, cx| map.snapshot(cx))
            .text()
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.display_map.read(cx).masked != masked {
            self.display_map.update(cx, |map, _| map.masked = masked);
        }
        cx.notify()
    }
}
