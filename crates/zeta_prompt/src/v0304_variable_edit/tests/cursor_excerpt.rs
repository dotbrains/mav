use super::*;

#[test]
fn test_write_cursor_excerpt_section() {
    let path = Path::new("test.rs");
    let context = "fn main() {\n    hello();\n}\n";
    let cursor_offset = 17;
    let mut prompt = String::new();
    write_cursor_excerpt_section(&mut prompt, path, context, cursor_offset);
    assert_eq!(
        prompt,
        "<|file_sep|>test.rs\nfn main() {\n    h<|user_cursor|>ello();\n}\n<|fim_prefix|>\n"
    );
}
