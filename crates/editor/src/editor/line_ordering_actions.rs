use super::*;

impl Editor {
    pub fn sort_lines_case_sensitive(
        &mut self,
        _: &SortLinesCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_immutable_lines(window, cx, |lines| lines.sort())
    }

    pub fn sort_lines_by_length(
        &mut self,
        _: &SortLinesByLength,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_immutable_lines(window, cx, |lines| {
            lines.sort_by_key(|&line| line.chars().count())
        })
    }

    pub fn sort_lines_case_insensitive(
        &mut self,
        _: &SortLinesCaseInsensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_immutable_lines(window, cx, |lines| {
            lines.sort_by_key(|line| line.to_lowercase())
        })
    }

    pub fn unique_lines_case_insensitive(
        &mut self,
        _: &UniqueLinesCaseInsensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_immutable_lines(window, cx, |lines| {
            let mut seen = HashSet::default();
            lines.retain(|line| seen.insert(line.to_lowercase()));
        })
    }

    pub fn unique_lines_case_sensitive(
        &mut self,
        _: &UniqueLinesCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.manipulate_immutable_lines(window, cx, |lines| {
            let mut seen = HashSet::default();
            lines.retain(|line| seen.insert(*line));
        })
    }

    pub fn reverse_lines(&mut self, _: &ReverseLines, window: &mut Window, cx: &mut Context<Self>) {
        self.manipulate_immutable_lines(window, cx, |lines| lines.reverse())
    }

    pub fn shuffle_lines(&mut self, _: &ShuffleLines, window: &mut Window, cx: &mut Context<Self>) {
        self.manipulate_immutable_lines(window, cx, |lines| lines.shuffle(&mut rand::rng()))
    }
}
