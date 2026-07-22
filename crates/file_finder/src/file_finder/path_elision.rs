use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathComponentSlice<'a> {
    path: Cow<'a, Path>,
    path_str: Cow<'a, str>,
    component_ranges: Vec<(Component<'a>, Range<usize>)>,
}

impl<'a> PathComponentSlice<'a> {
    pub(super) fn new(path: &'a str) -> Self {
        let trimmed_path = Path::new(path).components().as_path().as_os_str();
        let mut component_ranges = Vec::new();
        let mut components = Path::new(trimmed_path).components();
        let len = trimmed_path.as_encoded_bytes().len();
        let mut pos = 0;
        while let Some(component) = components.next() {
            component_ranges.push((component, pos..0));
            pos = len - components.as_path().as_os_str().as_encoded_bytes().len();
        }
        for ((_, range), ancestor) in component_ranges
            .iter_mut()
            .rev()
            .zip(Path::new(trimmed_path).ancestors())
        {
            range.end = ancestor.as_os_str().as_encoded_bytes().len();
        }
        Self {
            path: Cow::Borrowed(Path::new(path)),
            path_str: Cow::Borrowed(path),
            component_ranges,
        }
    }

    pub(super) fn elision_range(&self, budget: usize, matches: &[usize]) -> Option<Range<usize>> {
        let eligible_range = {
            assert!(matches.windows(2).all(|w| w[0] <= w[1]));
            let mut matches = matches.iter().copied().peekable();
            let mut longest: Option<Range<usize>> = None;
            let mut cur = 0..0;
            let mut seen_normal = false;
            for (i, (component, range)) in self.component_ranges.iter().enumerate() {
                let is_normal = matches!(component, Component::Normal(_));
                let is_first_normal = is_normal && !seen_normal;
                seen_normal |= is_normal;
                let is_last = i == self.component_ranges.len() - 1;
                let contains_match = matches.peek().is_some_and(|mat| range.contains(mat));
                if contains_match {
                    matches.next();
                }
                if is_first_normal || is_last || !is_normal || contains_match {
                    if longest
                        .as_ref()
                        .is_none_or(|old| old.end - old.start <= cur.end - cur.start)
                    {
                        longest = Some(cur);
                    }
                    cur = i + 1..i + 1;
                } else {
                    cur.end = i + 1;
                }
            }
            if longest
                .as_ref()
                .is_none_or(|old| old.end - old.start <= cur.end - cur.start)
            {
                longest = Some(cur);
            }
            longest
        };

        let eligible_range = eligible_range?;
        assert!(eligible_range.start <= eligible_range.end);
        if eligible_range.is_empty() {
            return None;
        }

        let elided_range: Range<usize> = {
            let byte_range = self.component_ranges[eligible_range.start].1.start
                ..self.component_ranges[eligible_range.end - 1].1.end;
            let midpoint = self.path_str.len() / 2;
            let distance_from_start = byte_range.start.abs_diff(midpoint);
            let distance_from_end = byte_range.end.abs_diff(midpoint);
            let pick_from_end = distance_from_start > distance_from_end;
            let mut len_with_elision = self.path_str.len();
            let mut i = eligible_range.start;
            while i < eligible_range.end {
                let x = if pick_from_end {
                    eligible_range.end - i + eligible_range.start - 1
                } else {
                    i
                };
                len_with_elision -= self.component_ranges[x]
                    .0
                    .as_os_str()
                    .as_encoded_bytes()
                    .len()
                    + 1;
                if len_with_elision <= budget {
                    break;
                }
                i += 1;
            }
            if len_with_elision > budget {
                return None;
            } else if pick_from_end {
                let x = eligible_range.end - i + eligible_range.start - 1;
                x..eligible_range.end
            } else {
                let x = i;
                eligible_range.start..x + 1
            }
        };

        let byte_range = self.component_ranges[elided_range.start].1.start
            ..self.component_ranges[elided_range.end - 1].1.end;
        Some(byte_range)
    }
}
