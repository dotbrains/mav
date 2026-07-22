use super::*;

pub(super) fn confirm_completion_callback<T: PromptCompletionProviderDelegate>(
    crease_text: SharedString,
    start: Anchor,
    content_len: usize,
    mention_uri: MentionUri,
    source: Arc<T>,
    editor: WeakEntity<Editor>,
    mention_set: WeakEntity<MentionSet>,
    workspace: Entity<Workspace>,
) -> Arc<dyn Fn(CompletionIntent, &mut Window, &mut App) -> bool + Send + Sync> {
    Arc::new(move |_, window, cx| {
        let source = source.clone();
        let editor = editor.clone();
        let mention_set = mention_set.clone();
        let crease_text = crease_text.clone();
        let mention_uri = mention_uri.clone();
        let workspace = workspace.clone();
        window.defer(cx, move |window, cx| {
            if let Some(editor) = editor.upgrade() {
                mention_set
                    .clone()
                    .update(cx, |mention_set, cx| {
                        mention_set
                            .confirm_mention_completion(
                                crease_text,
                                start,
                                content_len,
                                mention_uri,
                                source.supports_images(cx),
                                editor,
                                &workspace,
                                window,
                                cx,
                            )
                            .detach();
                    })
                    .ok();
            }
        });
        false
    })
}
