use super::*;

#[derive(Default)]
pub struct GitBlameStatus {
    text: Option<SharedString>,
    active_editor: Option<Entity<Editor>>,
    _subscriptions: Vec<Subscription>,
}

impl GitBlameStatus {
    fn update(&mut self, editor: Entity<Editor>, _window: &mut Window, cx: &mut Context<Self>) {
        let inline_blame = ProjectSettings::get_global(cx).git.inline_blame;
        let text =
            if inline_blame.enabled && inline_blame.location == InlineBlameLocation::StatusBar {
                editor
                    .update(cx, |editor, cx| editor.active_git_blame_entry(cx))
                    .map(|blame_entry| SharedString::from(format_blame_text(&blame_entry, cx)))
            } else {
                None
            };

        if text != self.text {
            self.text = text;
            cx.notify();
        }
    }
}

impl Render for GitBlameStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inline_blame = ProjectSettings::get_global(cx).git.inline_blame;
        if !inline_blame.enabled || inline_blame.location != InlineBlameLocation::StatusBar {
            return div();
        }

        div().when_some(self.text.clone(), |el, text| {
            el.child(
                Button::new("git-blame-status", text.clone())
                    .label_size(LabelSize::Small)
                    .start_icon(
                        Icon::new(IconName::FileGit)
                            .size(IconSize::Small)
                            .color(Color::Hint),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(editor) = this.active_editor.clone() {
                            let focus_handle = gpui::Focusable::focus_handle(editor.read(cx), cx);
                            focus_handle.dispatch_action(
                                &editor::actions::OpenGitBlameCommit,
                                window,
                                cx,
                            );
                        }
                    }))
                    .tooltip(ui::Tooltip::text(text)),
            )
        })
    }
}

impl workspace::StatusItemView for GitBlameStatus {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn workspace::item::ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = active_pane_item.and_then(|item| item.act_as::<Editor>(cx)) {
            self.active_editor = Some(editor.clone());
            self._subscriptions = vec![cx.observe_in(&editor, window, Self::update)];
            self.update(editor, window, cx);
        } else {
            self.text = None;
            self.active_editor = None;
            self._subscriptions.clear();
            cx.notify();
        }
    }

    fn hide_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        None
    }
}
