use crate::Edit;
use std::{
    cmp, mem,
    ops::{Add, AddAssign, Sub},
};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Patch<T>(Vec<Edit<T>>);

impl<T> Patch<T>
where
    T: 'static + Clone + Copy + Ord + Default,
{
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn new(edits: Vec<Edit<T>>) -> Self {
        #[cfg(debug_assertions)]
        {
            let mut last_edit: Option<&Edit<T>> = None;
            for edit in &edits {
                if let Some(last_edit) = last_edit {
                    assert!(edit.old.start > last_edit.old.end);
                    assert!(edit.new.start > last_edit.new.end);
                }
                last_edit = Some(edit);
            }
        }
        Self(edits)
    }

    pub fn edits(&self) -> &[Edit<T>] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<Edit<T>> {
        self.0
    }
    pub fn invert(&mut self) -> &mut Self {
        for edit in &mut self.0 {
            mem::swap(&mut edit.old, &mut edit.new);
        }
        self
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, edit: Edit<T>) {
        if edit.is_empty() {
            return;
        }
        self.push_maybe_empty(edit);
    }

    pub fn push_maybe_empty(&mut self, edit: Edit<T>) {
        if let Some(last) = self.0.last_mut() {
            if last.old.end >= edit.old.start {
                last.old.end = edit.old.end;
                last.new.end = edit.new.end;
            } else {
                self.0.push(edit);
            }
        } else {
            self.0.push(edit);
        }
    }
}

impl<T, TDelta> Patch<T>
where
    T: 'static
        + Copy
        + Ord
        + Sub<T, Output = TDelta>
        + Add<TDelta, Output = T>
        + AddAssign<TDelta>
        + Default,
    TDelta: Ord + Copy,
{
    #[must_use]
    pub fn compose(&self, new_edits_iter: impl IntoIterator<Item = Edit<T>>) -> Self {
        let mut old_edits_iter = self.0.iter().cloned().peekable();
        let mut new_edits_iter = new_edits_iter.into_iter().peekable();
        let mut composed = Patch(Vec::new());

        let mut old_start = T::default();
        let mut new_start = T::default();
        loop {
            let old_edit = old_edits_iter.peek_mut();
            let new_edit = new_edits_iter.peek_mut();

            // Push the old edit if its new end is before the new edit's old start.
            if let Some(old_edit) = old_edit.as_ref() {
                let new_edit = new_edit.as_ref();
                if new_edit.is_none_or(|new_edit| old_edit.new.end < new_edit.old.start) {
                    let catchup = old_edit.old.start - old_start;
                    old_start += catchup;
                    new_start += catchup;

                    let old_end = old_start + old_edit.old_len();
                    let new_end = new_start + old_edit.new_len();
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });
                    old_start = old_end;
                    new_start = new_end;
                    old_edits_iter.next();
                    continue;
                }
            }

            // Push the new edit if its old end is before the old edit's new start.
            if let Some(new_edit) = new_edit.as_ref() {
                let old_edit = old_edit.as_ref();
                if old_edit.is_none_or(|old_edit| new_edit.old.end < old_edit.new.start) {
                    let catchup = new_edit.new.start - new_start;
                    old_start += catchup;
                    new_start += catchup;

                    let old_end = old_start + new_edit.old_len();
                    let new_end = new_start + new_edit.new_len();
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });
                    old_start = old_end;
                    new_start = new_end;
                    new_edits_iter.next();
                    continue;
                }
            }

            // If we still have edits by this point then they must intersect, so we compose them.
            if let Some((old_edit, new_edit)) = old_edit.zip(new_edit) {
                if old_edit.new.start < new_edit.old.start {
                    let catchup = old_edit.old.start - old_start;
                    old_start += catchup;
                    new_start += catchup;

                    let overshoot = new_edit.old.start - old_edit.new.start;
                    let old_end = cmp::min(old_start + overshoot, old_edit.old.end);
                    let new_end = new_start + overshoot;
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });

                    old_edit.old.start = old_end;
                    old_edit.new.start += overshoot;
                    old_start = old_end;
                    new_start = new_end;
                } else {
                    let catchup = new_edit.new.start - new_start;
                    old_start += catchup;
                    new_start += catchup;

                    let overshoot = old_edit.new.start - new_edit.old.start;
                    let old_end = old_start + overshoot;
                    let new_end = cmp::min(new_start + overshoot, new_edit.new.end);
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });

                    new_edit.old.start += overshoot;
                    new_edit.new.start = new_end;
                    old_start = old_end;
                    new_start = new_end;
                }

                if old_edit.new.end > new_edit.old.end {
                    let old_end = old_start + cmp::min(old_edit.old_len(), new_edit.old_len());
                    let new_end = new_start + new_edit.new_len();
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });

                    old_edit.old.start = old_end;
                    old_edit.new.start = new_edit.old.end;
                    old_start = old_end;
                    new_start = new_end;
                    new_edits_iter.next();
                } else {
                    let old_end = old_start + old_edit.old_len();
                    let new_end = new_start + cmp::min(old_edit.new_len(), new_edit.new_len());
                    composed.push(Edit {
                        old: old_start..old_end,
                        new: new_start..new_end,
                    });

                    new_edit.old.start = old_edit.new.end;
                    new_edit.new.start = new_end;
                    old_start = old_end;
                    new_start = new_end;
                    old_edits_iter.next();
                }
            } else {
                break;
            }
        }

        composed
    }

    pub fn old_to_new(&self, old: T) -> T {
        let ix = match self.0.binary_search_by(|probe| probe.old.start.cmp(&old)) {
            Ok(ix) => ix,
            Err(ix) => {
                if ix == 0 {
                    return old;
                } else {
                    ix - 1
                }
            }
        };
        if let Some(edit) = self.0.get(ix) {
            if old >= edit.old.end {
                edit.new.end + (old - edit.old.end)
            } else {
                edit.new.start
            }
        } else {
            old
        }
    }

    /// Returns the edit that touches the given old position.
    ///
    /// An edit is considered to touch the given old position if edit.old.start <= old <= edit.old.end (note, inclusive on the right).
    ///
    /// If there are no edits touching the given old position, an empty edit with appropriate (empty) old and new ranges is returned.
    pub fn edit_for_old_position(&self, old: T) -> Edit<T> {
        let edits = self.edits();

        let ix = match edits.binary_search_by(|probe| probe.old.start.cmp(&old)) {
            Ok(ix) => ix,
            Err(ix) => {
                if ix == 0 {
                    return Edit {
                        old: old..old,
                        new: old..old,
                    };
                } else {
                    ix - 1
                }
            }
        };

        if let Some(edit) = edits.get(ix) {
            if old > edit.old.end {
                let translated = edit.new.end + (old - edit.old.end);
                Edit {
                    new: translated..translated,
                    old: old..old,
                }
            } else {
                edit.clone()
            }
        } else {
            Edit {
                old: old..old,
                new: old..old,
            }
        }
    }
}

impl<T> Patch<T> {
    pub fn retain_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut Edit<T>) -> bool,
    {
        self.0.retain_mut(f);
    }
}

impl<T: Clone> IntoIterator for Patch<T> {
    type Item = Edit<T>;
    type IntoIter = std::vec::IntoIter<Edit<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a Patch<T> {
    type Item = Edit<T>;
    type IntoIter = std::iter::Cloned<std::slice::Iter<'a, Edit<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().cloned()
    }
}

impl<'a, T: Clone> IntoIterator for &'a mut Patch<T> {
    type Item = Edit<T>;
    type IntoIter = std::iter::Cloned<std::slice::Iter<'a, Edit<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().cloned()
    }
}

#[cfg(test)]
#[path = "patch/tests.rs"]
mod tests;
