use futures::future::Shared;
use gpui::{Keystroke, Task};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub(super) struct DispatchingKeystrokes {
    pub(super) dispatched: HashSet<Vec<Keystroke>>,
    pub(super) queue: VecDeque<Keystroke>,
    pub(super) task: Option<Shared<Task<()>>>,
}
