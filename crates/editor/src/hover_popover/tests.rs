mod tests {
    use super::*;
    use crate::{
        PointForPosition,
        actions::ConfirmCompletion,
        editor_tests::{handle_completion_request, init_test},
        inlays::inlay_hints::tests::{cached_hint_labels, visible_hint_labels},
        test::editor_lsp_test_context::EditorLspTestContext,
    };
    use collections::BTreeSet;
    use futures::stream::StreamExt;
    use gpui::App;
    use indoc::indoc;
    use markdown::parser::MarkdownEvent;
    use project::InlayId;
    use settings::InlayHintSettingsContent;
    use settings::{DelayMs, SettingsStore};
    use std::sync::atomic;
    use std::sync::atomic::AtomicUsize;
    use text::Bias;

    fn get_hover_popover_delay(cx: &gpui::TestAppContext) -> u64 {
        cx.read(|cx: &App| -> u64 { EditorSettings::get_global(cx).hover_popover_delay.0 })
    }

    impl InfoPopover {
        fn get_rendered_text(&self, cx: &gpui::App) -> String {
            let mut rendered_text = String::new();
            if let Some(parsed_content) = self.parsed_content.clone() {
                let markdown = parsed_content.read(cx);
                let text = markdown.parsed_markdown().source().to_string();
                let data = markdown.parsed_markdown().events();
                let slice = data;

                for (range, event) in slice.iter() {
                    match event {
                        MarkdownEvent::SubstitutedText(parsed) => {
                            rendered_text.push_str(parsed.as_str())
                        }
                        MarkdownEvent::Text | MarkdownEvent::Code => {
                            rendered_text.push_str(&text[range.clone()])
                        }
                        _ => {}
                    }
                }
            }
            rendered_text
        }
    }

    mod content;
    mod inlay;
    mod markdown;
    mod mouse;
    mod sticky;
}
