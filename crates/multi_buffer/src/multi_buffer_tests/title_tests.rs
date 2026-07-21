use super::*;

#[gpui::test]
fn test_new_empty_buffer_uses_untitled_title(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), "untitled");
}

#[gpui::test]
fn test_new_empty_buffer_uses_untitled_title_when_only_contains_whitespace(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("\n ", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), "untitled");
}

#[gpui::test]
fn test_new_empty_buffer_takes_first_line_for_title(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("Hello World\nSecond line", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), "Hello World");
}

#[gpui::test]
fn test_new_empty_buffer_takes_trimmed_first_line_for_title(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("\nHello, World ", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), "Hello, World");
}

#[gpui::test]
fn test_new_empty_buffer_uses_truncated_first_line_for_title(cx: &mut App) {
    let title = "aaaaaaaaaabbbbbbbbbbccccccccccddddddddddeeeeeeeeee";
    let title_after = "aaaaaaaaaabbbbbbbbbbccccccccccdddddddddd";
    let buffer = cx.new(|cx| Buffer::local(title, cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), title_after);
}

#[gpui::test]
fn test_new_empty_buffer_uses_truncated_first_line_for_title_after_merging_adjacent_spaces(
    cx: &mut App,
) {
    let title = "aaaaaaaaaabbbbbbbbbb    ccccccccccddddddddddeeeeeeeeee";
    let title_after = "aaaaaaaaaabbbbbbbbbb ccccccccccddddddddd";
    let buffer = cx.new(|cx| Buffer::local(title, cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    assert_eq!(multibuffer.read(cx).title(cx), title_after);
}

#[gpui::test]
fn test_new_empty_buffers_title_can_be_set(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("Hello World", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
    assert_eq!(multibuffer.read(cx).title(cx), "Hello World");

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_title("Hey".into(), cx)
    });
    assert_eq!(multibuffer.read(cx).title(cx), "Hey");
}
