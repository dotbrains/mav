use gpui::{Context, Window};
use std::time::{Duration, Instant};

pub(super) const COPIED_STATE_DURATION: Duration = Duration::from_secs(2);

pub(super) struct CopiedState {
    copied_at: Option<Instant>,
}

impl CopiedState {
    pub(super) fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { copied_at: None }
    }

    pub(super) fn is_copied(&self) -> bool {
        self.copied_at
            .map(|t| t.elapsed() < COPIED_STATE_DURATION)
            .unwrap_or(false)
    }

    pub(super) fn mark_copied(&mut self) {
        self.copied_at = Some(Instant::now());
    }
}
