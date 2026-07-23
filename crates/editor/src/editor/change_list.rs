use super::*;

struct ChangeLocation {
    current: Option<Vec<Anchor>>,
    original: Vec<Anchor>,
}

impl ChangeLocation {
    pub(crate) fn locations(&self) -> &[Anchor] {
        self.current.as_ref().unwrap_or(&self.original)
    }
}

/// A set of caret positions, registered when the editor was edited.
pub struct ChangeList {
    changes: Vec<ChangeLocation>,
    /// Currently "selected" change.
    position: Option<usize>,
}

impl ChangeList {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
            position: None,
        }
    }

    /// Moves to the next change in the list (based on the direction given) and returns the caret positions for the next change.
    /// If reaches the end of the list in the direction, returns the corresponding change until called for a different direction.
    pub fn next_change(&mut self, count: usize, direction: Direction) -> Option<&[Anchor]> {
        if self.changes.is_empty() {
            return None;
        }

        let prev = self.position.unwrap_or(self.changes.len());
        let next = if direction == Direction::Prev {
            prev.saturating_sub(count)
        } else {
            (prev + count).min(self.changes.len() - 1)
        };
        self.position = Some(next);
        self.changes.get(next).map(|change| change.locations())
    }

    /// Adds a new change to the list, resetting the change list position.
    pub fn push_to_change_list(&mut self, group: bool, new_positions: Vec<Anchor>) {
        self.position.take();
        if let Some(last) = self.changes.last_mut()
            && group
        {
            last.current = Some(new_positions)
        } else {
            self.changes.push(ChangeLocation {
                original: new_positions,
                current: None,
            });
        }
    }

    pub fn last(&self) -> Option<&[Anchor]> {
        self.changes.last().map(|change| change.locations())
    }

    pub fn last_before_grouping(&self) -> Option<&[Anchor]> {
        self.changes.last().map(|change| change.original.as_slice())
    }

    pub fn invert_last_group(&mut self) {
        if let Some(last) = self.changes.last_mut()
            && let Some(current) = last.current.as_mut()
        {
            mem::swap(&mut last.original, current);
        }
    }
}
