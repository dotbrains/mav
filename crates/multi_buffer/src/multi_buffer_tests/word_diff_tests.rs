use super::*;

fn collect_word_diffs(
    base_text: &str,
    modified_text: &str,
    cx: &mut TestAppContext,
) -> Vec<String> {
    let buffer = cx.new(|cx| Buffer::local(modified_text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(buffer.clone(), cx);
        multibuffer.add_diff(diff.clone(), cx);
        multibuffer
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    let snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    let text = snapshot.text();

    snapshot
        .diff_hunks()
        .flat_map(|hunk| hunk.word_diffs)
        .map(|range| text[range.start.0..range.end.0].to_string())
        .collect()
}

#[gpui::test]
async fn test_word_diff_simple_replacement(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| SettingsStore::test(cx));
    cx.set_global(settings_store);

    let base_text = "hello world foo bar\n";
    let modified_text = "hello WORLD foo BAR\n";

    let word_diffs = collect_word_diffs(base_text, modified_text, cx);

    assert_eq!(word_diffs, vec!["world", "bar", "WORLD", "BAR"]);
}

#[gpui::test]
async fn test_word_diff_white_space(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| SettingsStore::test(cx));
    cx.set_global(settings_store);

    let base_text = "hello world foo bar\n";
    let modified_text = "    hello world foo bar\n";

    let word_diffs = collect_word_diffs(base_text, modified_text, cx);

    assert_eq!(word_diffs, vec!["    "]);
}

#[gpui::test]
async fn test_word_diff_consecutive_modified_lines(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| SettingsStore::test(cx));
    cx.set_global(settings_store);

    let base_text = "aaa bbb\nccc ddd\n";
    let modified_text = "aaa BBB\nccc DDD\n";

    let word_diffs = collect_word_diffs(base_text, modified_text, cx);

    assert_eq!(
        word_diffs,
        vec!["bbb", "ddd", "BBB", "DDD"],
        "consecutive modified lines should produce word diffs when line counts match"
    );
}

#[gpui::test]
async fn test_word_diff_modified_lines_with_deletion_between(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| SettingsStore::test(cx));
    cx.set_global(settings_store);

    let base_text = "aaa bbb\ndeleted line\nccc ddd\n";
    let modified_text = "aaa BBB\nccc DDD\n";

    let word_diffs = collect_word_diffs(base_text, modified_text, cx);

    assert_eq!(
        word_diffs,
        Vec::<String>::new(),
        "modified lines with a deleted line between should not produce word diffs"
    );
}

#[gpui::test]
async fn test_word_diff_disabled(cx: &mut TestAppContext) {
    let settings_store = cx.update(|cx| {
        let mut settings_store = SettingsStore::test(cx);
        settings_store.update_user_settings(cx, |settings| {
            settings.project.all_languages.defaults.word_diff_enabled = Some(false);
        });
        settings_store
    });
    cx.set_global(settings_store);

    let base_text = "hello world\n";
    let modified_text = "hello WORLD\n";

    let word_diffs = collect_word_diffs(base_text, modified_text, cx);

    assert_eq!(
        word_diffs,
        Vec::<String>::new(),
        "word diffs should be empty when disabled"
    );
}
