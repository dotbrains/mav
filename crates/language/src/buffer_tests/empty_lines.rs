use super::*;

#[gpui::test]
fn test_insert_empty_line(cx: &mut App) {
    init_settings(cx, |_| {});

    // Insert empty line at the beginning, requesting an empty line above
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(0, 0), true, false, cx);
        assert_eq!(buffer.text(), "\nabc\ndef\nghi");
        assert_eq!(point, Point::new(0, 0));
        buffer
    });

    // Insert empty line at the beginning, requesting an empty line above and below
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(0, 0), true, true, cx);
        assert_eq!(buffer.text(), "\n\nabc\ndef\nghi");
        assert_eq!(point, Point::new(0, 0));
        buffer
    });

    // Insert empty line at the start of a line, requesting empty lines above and below
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(2, 0), true, true, cx);
        assert_eq!(buffer.text(), "abc\ndef\n\n\n\nghi");
        assert_eq!(point, Point::new(3, 0));
        buffer
    });

    // Insert empty line in the middle of a line, requesting empty lines above and below
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndefghi\njkl", cx);
        let point = buffer.insert_empty_line(Point::new(1, 3), true, true, cx);
        assert_eq!(buffer.text(), "abc\ndef\n\n\n\nghi\njkl");
        assert_eq!(point, Point::new(3, 0));
        buffer
    });

    // Insert empty line in the middle of a line, requesting empty line above only
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndefghi\njkl", cx);
        let point = buffer.insert_empty_line(Point::new(1, 3), true, false, cx);
        assert_eq!(buffer.text(), "abc\ndef\n\n\nghi\njkl");
        assert_eq!(point, Point::new(3, 0));
        buffer
    });

    // Insert empty line in the middle of a line, requesting empty line below only
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndefghi\njkl", cx);
        let point = buffer.insert_empty_line(Point::new(1, 3), false, true, cx);
        assert_eq!(buffer.text(), "abc\ndef\n\n\nghi\njkl");
        assert_eq!(point, Point::new(2, 0));
        buffer
    });

    // Insert empty line at the end, requesting empty lines above and below
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(2, 3), true, true, cx);
        assert_eq!(buffer.text(), "abc\ndef\nghi\n\n\n");
        assert_eq!(point, Point::new(4, 0));
        buffer
    });

    // Insert empty line at the end, requesting empty line above only
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(2, 3), true, false, cx);
        assert_eq!(buffer.text(), "abc\ndef\nghi\n\n");
        assert_eq!(point, Point::new(4, 0));
        buffer
    });

    // Insert empty line at the end, requesting empty line below only
    cx.new(|cx| {
        let mut buffer = Buffer::local("abc\ndef\nghi", cx);
        let point = buffer.insert_empty_line(Point::new(2, 3), false, true, cx);
        assert_eq!(buffer.text(), "abc\ndef\nghi\n\n");
        assert_eq!(point, Point::new(3, 0));
        buffer
    });
}
