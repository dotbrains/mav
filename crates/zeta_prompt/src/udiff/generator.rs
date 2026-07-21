use std::{fmt::Write, ops::Range};

use imara_diff::{
    Algorithm, Sink, diff,
    intern::{InternedInput, Interner, Token},
};

pub fn unified_diff_with_context(
    old_text: &str,
    new_text: &str,
    old_start_line: u32,
    new_start_line: u32,
    context_lines: u32,
) -> String {
    let input = InternedInput::new(old_text, new_text);
    diff(
        Algorithm::Histogram,
        &input,
        OffsetUnifiedDiffBuilder::new(&input, old_start_line, new_start_line, context_lines),
    )
}

struct OffsetUnifiedDiffBuilder<'a> {
    before: &'a [Token],
    after: &'a [Token],
    interner: &'a Interner<&'a str>,
    pos: u32,
    before_hunk_start: u32,
    after_hunk_start: u32,
    before_hunk_len: u32,
    after_hunk_len: u32,
    old_line_offset: u32,
    new_line_offset: u32,
    context_lines: u32,
    buffer: String,
    dst: String,
}

impl<'a> OffsetUnifiedDiffBuilder<'a> {
    fn new(
        input: &'a InternedInput<&'a str>,
        old_line_offset: u32,
        new_line_offset: u32,
        context_lines: u32,
    ) -> Self {
        Self {
            before_hunk_start: 0,
            after_hunk_start: 0,
            before_hunk_len: 0,
            after_hunk_len: 0,
            old_line_offset,
            new_line_offset,
            context_lines,
            buffer: String::with_capacity(8),
            dst: String::new(),
            interner: &input.interner,
            before: &input.before,
            after: &input.after,
            pos: 0,
        }
    }

    fn print_tokens(&mut self, tokens: &[Token], prefix: char) {
        for &token in tokens {
            writeln!(&mut self.buffer, "{prefix}{}", self.interner[token]).unwrap();
        }
    }

    fn flush(&mut self) {
        if self.before_hunk_len == 0 && self.after_hunk_len == 0 {
            return;
        }

        let end = (self.pos + self.context_lines).min(self.before.len() as u32);
        self.update_pos(end, end);

        writeln!(
            &mut self.dst,
            "@@ -{},{} +{},{} @@",
            self.before_hunk_start + 1 + self.old_line_offset,
            self.before_hunk_len,
            self.after_hunk_start + 1 + self.new_line_offset,
            self.after_hunk_len,
        )
        .unwrap();
        write!(&mut self.dst, "{}", &self.buffer).unwrap();
        self.buffer.clear();
        self.before_hunk_len = 0;
        self.after_hunk_len = 0;
    }

    fn update_pos(&mut self, print_to: u32, move_to: u32) {
        self.print_tokens(&self.before[self.pos as usize..print_to as usize], ' ');
        let len = print_to - self.pos;
        self.before_hunk_len += len;
        self.after_hunk_len += len;
        self.pos = move_to;
    }
}

impl Sink for OffsetUnifiedDiffBuilder<'_> {
    type Out = String;

    fn process_change(&mut self, before: Range<u32>, after: Range<u32>) {
        if before.start - self.pos > self.context_lines * 2 {
            self.flush();
        }
        if self.before_hunk_len == 0 && self.after_hunk_len == 0 {
            self.pos = before.start.saturating_sub(self.context_lines);
            self.before_hunk_start = self.pos;
            self.after_hunk_start = after.start.saturating_sub(self.context_lines);
        }

        self.update_pos(before.start, before.end);
        self.before_hunk_len += before.end - before.start;
        self.after_hunk_len += after.end - after.start;
        self.print_tokens(
            &self.before[before.start as usize..before.end as usize],
            '-',
        );
        self.print_tokens(&self.after[after.start as usize..after.end as usize], '+');
    }

    fn finish(mut self) -> Self::Out {
        self.flush();
        self.dst
    }
}
