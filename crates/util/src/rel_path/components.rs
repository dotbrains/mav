use super::RelPath;

const SEPARATOR: char = '/';

#[derive(Default)]
pub struct RelPathComponents<'a>(pub(super) &'a str);

pub struct RelPathAncestors<'a>(pub(super) Option<&'a str>);

impl<'a> RelPathComponents<'a> {
    pub fn rest(&self) -> &'a RelPath {
        RelPath::new_unchecked(self.0)
    }
}

impl<'a> Iterator for RelPathComponents<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sep_ix) = self.0.find(SEPARATOR) {
            let (head, tail) = self.0.split_at(sep_ix);
            self.0 = &tail[1..];
            Some(head)
        } else if self.0.is_empty() {
            None
        } else {
            let result = self.0;
            self.0 = "";
            Some(result)
        }
    }
}

impl<'a> Iterator for RelPathAncestors<'a> {
    type Item = &'a RelPath;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.0?;
        if let Some(sep_ix) = result.rfind(SEPARATOR) {
            self.0 = Some(&result[..sep_ix]);
        } else if !result.is_empty() {
            self.0 = Some("");
        } else {
            self.0 = None;
        }
        Some(RelPath::new_unchecked(result))
    }
}

impl<'a> DoubleEndedIterator for RelPathComponents<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(sep_ix) = self.0.rfind(SEPARATOR) {
            let (head, tail) = self.0.split_at(sep_ix);
            self.0 = head;
            Some(&tail[1..])
        } else if self.0.is_empty() {
            None
        } else {
            let result = self.0;
            self.0 = "";
            Some(result)
        }
    }
}
