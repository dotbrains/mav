use super::*;

struct TerminalProvider(Entity<TerminalPanel>);

impl workspace::TerminalProvider for TerminalProvider {
    fn spawn(
        &self,
        task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Option<Result<ExitStatus>>> {
        let terminal_panel = self.0.clone();
        window.spawn(cx, async move |cx| {
            let terminal = terminal_panel
                .update_in(cx, |terminal_panel, window, cx| {
                    terminal_panel.spawn_task(&task, window, cx)
                })
                .ok()?
                .await;
            match terminal {
                Ok(terminal) => {
                    let exit_status = terminal
                        .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
                        .ok()?
                        .await?;
                    Some(Ok(exit_status))
                }
                Err(e) => Some(Err(e)),
            }
        })
    }
}

struct InlineAssistTabBarButton {
    focus_handle: FocusHandle,
}

impl Render for InlineAssistTabBarButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        IconButton::new("terminal_inline_assistant", IconName::MavAssistant)
            .icon_size(IconSize::Small)
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(InlineAssist::default().boxed_clone(), cx);
            }))
            .tooltip(move |_window, cx| {
                Tooltip::for_action_in("Inline Assist", &InlineAssist::default(), &focus_handle, cx)
            })
    }
}
