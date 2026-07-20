use super::*;

impl Pane {
    pub fn display_nav_history_buttons(&mut self, display: Option<bool>) {
        self.display_nav_history_buttons = display;
    }

    pub(super) fn pinned_item_ids(&self) -> Vec<EntityId> {
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if self.is_tab_pinned(index) {
                    return Some(item.item_id());
                }

                None
            })
            .collect()
    }

    pub(super) fn clean_item_ids(&self, cx: &mut Context<Pane>) -> Vec<EntityId> {
        self.items()
            .filter_map(|item| {
                if !item.is_dirty(cx) {
                    return Some(item.item_id());
                }

                None
            })
            .collect()
    }

    pub(super) fn to_the_side_item_ids(&self, item_id: EntityId, side: Side) -> Vec<EntityId> {
        match side {
            Side::Left => self
                .items()
                .take_while(|item| item.item_id() != item_id)
                .map(|item| item.item_id())
                .collect(),
            Side::Right => self
                .items()
                .rev()
                .take_while(|item| item.item_id() != item_id)
                .map(|item| item.item_id())
                .collect(),
        }
    }

    pub(super) fn multibuffer_item_ids(&self, cx: &mut Context<Pane>) -> Vec<EntityId> {
        self.items()
            .filter(|item| item.buffer_kind(cx) == ItemBufferKind::Multibuffer)
            .map(|item| item.item_id())
            .collect()
    }

    pub fn drag_split_direction(&self) -> Option<SplitDirection> {
        self.drag_split_direction
    }

    pub(super) fn render_pane_drag_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("pane_drag_handle")
            .absolute()
            .top_0()
            .left_1_2()
            .w_16()
            .h(px(5.))
            .ml(rems(-2.))
            .cursor_move()
            .rounded_b_sm()
            .hover(|this| this.bg(cx.theme().colors().element_hover))
            .on_drag(
                DraggedPane { pane: cx.entity() },
                |dragged_pane, _, _, cx| cx.new(|_| dragged_pane.clone()),
            )
    }

    pub(super) fn render_header_with_traffic_light_spacer(
        &self,
        header: AnyElement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.should_reserve_traffic_light_space(window, cx) {
            return header;
        }

        let hidden_sidebar_controls = self.render_hidden_sidebar_header_controls(cx);

        h_flex()
            .h(Tab::container_height(cx))
            .w_full()
            .flex_none()
            .bg(cx.theme().colors().tab_bar_background)
            .child(ui::utils::traffic_light_spacer_with_child(
                cx,
                true,
                hidden_sidebar_controls,
            ))
            .child(div().h_full().min_w_0().flex_1().child(header))
            .into_any_element()
    }

    pub(super) fn render_hidden_sidebar_header_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let workspace = self.workspace.upgrade()?;
        let multi_workspace = workspace
            .read(cx)
            .multi_workspace()
            .cloned()
            .and_then(|multi_workspace| multi_workspace.upgrade())?;
        let sidebar = multi_workspace.read(cx).sidebar_render_state(cx);
        if sidebar.open {
            return None;
        }
        let current_pane = cx.entity();
        let project_pane_visible = if self.pane_kind == PaneKind::Project {
            self.is_visible()
        } else {
            workspace
                .read(cx)
                .panel_pane_visible_except(PaneKind::Project, &current_pane, cx)
        };

        render_sidebar_header_controls_with_project_pane_visibility(
            multi_workspace,
            sidebar,
            project_pane_visible,
            cx,
        )
    }

    pub fn set_zoom_out_on_close(&mut self, zoom_out_on_close: bool) {
        self.zoom_out_on_close = zoom_out_on_close;
    }
}
