use super::*;

#[derive(Default)]
pub(super) struct ParseState {
    pub(super) events: Vec<(Range<usize>, MarkdownEvent)>,
    pub(super) root_block_starts: Vec<usize>,
    depth: usize,
}

impl ParseState {
    pub(super) fn push_event(&mut self, range: Range<usize>, event: MarkdownEvent) {
        match &event {
            MarkdownEvent::Start(_) => {
                if self.depth == 0 {
                    self.root_block_starts.push(range.start);
                    self.events.push((range.clone(), MarkdownEvent::RootStart));
                }
                self.depth += 1;
                self.events.push((range, event));
            }
            MarkdownEvent::End(_) => {
                self.events.push((range.clone(), event));
                if self.depth > 0 {
                    self.depth -= 1;
                    if self.depth == 0 {
                        let root_block_index = self.root_block_starts.len() - 1;
                        self.events
                            .push((range, MarkdownEvent::RootEnd(root_block_index)));
                    }
                }
            }
            MarkdownEvent::Rule => {
                if self.depth == 0 && !range.is_empty() {
                    self.root_block_starts.push(range.start);
                    let root_block_index = self.root_block_starts.len() - 1;
                    self.events.push((range.clone(), MarkdownEvent::RootStart));
                    self.events.push((range.clone(), event));
                    self.events
                        .push((range, MarkdownEvent::RootEnd(root_block_index)));
                } else {
                    self.events.push((range, event));
                }
            }
            _ => {
                self.events.push((range, event));
            }
        }
    }
}
