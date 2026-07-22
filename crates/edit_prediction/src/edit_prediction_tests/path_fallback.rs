use super::*;

#[gpui::test]
fn test_buffer_path_with_id_fallback_for_untitled_buffers(cx: &mut TestAppContext) {
    let buffer_1 = cx.new(|cx| Buffer::local("one", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("two", cx));

    let snapshot_1 = buffer_1.read_with(cx, |buffer, _| buffer.text_snapshot());
    let snapshot_2 = buffer_2.read_with(cx, |buffer, _| buffer.text_snapshot());

    let path_1 = cx.read(|cx| buffer_path_with_id_fallback(None, &snapshot_1, cx));
    let path_2 = cx.read(|cx| buffer_path_with_id_fallback(None, &snapshot_2, cx));

    assert_eq!(
        path_1.as_ref(),
        Path::new(&format!("untitled-{}", snapshot_1.remote_id()))
    );
    assert_eq!(
        path_2.as_ref(),
        Path::new(&format!("untitled-{}", snapshot_2.remote_id()))
    );
    assert_ne!(path_1.as_ref(), path_2.as_ref());
}
