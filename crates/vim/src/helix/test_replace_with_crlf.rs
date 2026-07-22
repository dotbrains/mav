use super::*;

#[gpui::test]
async fn test_replace_with_crlf(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();
    cx.set_state("«xˇ»z", Mode::HelixNormal);

    let vim = cx.update_editor(|editor, _window, _cx| editor.addon::<VimAddon>().cloned().unwrap());
    cx.update(|window, cx| {
        vim.entity.update(cx, |vim, cx| {
            vim.helix_replace("a\r\nb", window, cx);
        });
    });

    cx.assert_state("«a\nbˇ»z", Mode::HelixNormal);
}
