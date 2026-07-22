use super::*;

pub(super) enum QueryState {
    Pending(SharedString),
    Confirmed((SharedString, Task<()>)),
    Empty,
}

impl QueryState {
    pub(super) fn next_state(&mut self) {
        match self {
            Self::Confirmed((query, _)) => *self = Self::Pending(std::mem::take(query)),
            _ => {}
        };
    }
}

pub(super) struct SearchState {
    pub(super) case_sensitive: bool,
    pub(super) editor: Entity<Editor>,
    pub(super) state: QueryState,
    pub(super) matches: IndexSet<Oid>,
    pub(super) selected_index: Option<usize>,
}

pub(super) struct SplitState {
    pub(super) left_ratio: f32,
    pub(super) visible_left_ratio: f32,
}

impl SplitState {
    pub(super) fn new() -> Self {
        Self {
            left_ratio: 1.0,
            visible_left_ratio: 1.0,
        }
    }

    pub(super) fn right_ratio(&self) -> f32 {
        1.0 - self.visible_left_ratio
    }

    pub(super) fn on_drag_move(
        &mut self,
        drag_event: &DragMoveEvent<DraggedSplitHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let drag_position = drag_event.event.position;
        let bounds = drag_event.bounds;
        let bounds_width = bounds.right() - bounds.left();

        let min_ratio = 0.1;
        let max_ratio = 0.9;

        let new_ratio = (drag_position.x - bounds.left()) / bounds_width;
        self.visible_left_ratio = new_ratio.clamp(min_ratio, max_ratio);
    }

    pub(super) fn commit_ratio(&mut self) {
        self.left_ratio = self.visible_left_ratio;
    }

    pub(super) fn on_double_click(&mut self) {
        self.left_ratio = 1.0;
        self.visible_left_ratio = 1.0;
    }
}
