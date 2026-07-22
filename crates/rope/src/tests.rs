use super::*;
use sum_tree::Bias;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod basic;
mod boundaries;
mod lines;
mod matches;
mod push_front;
mod random;

fn clip_offset(text: &str, mut offset: usize, bias: Bias) -> usize {
    while !text.is_char_boundary(offset) {
        match bias {
            Bias::Left => offset -= 1,
            Bias::Right => offset += 1,
        }
    }
    offset
}

impl Rope {
    fn text(&self) -> String {
        let mut text = String::new();
        for chunk in self.chunks.cursor::<()>(()) {
            text.push_str(&chunk.text);
        }
        text
    }
}
