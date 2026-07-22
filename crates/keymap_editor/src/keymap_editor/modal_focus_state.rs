use super::*;

pub(super) struct KeybindingEditorModalFocusState {
    handles: Vec<FocusHandle>,
}

impl KeybindingEditorModalFocusState {
    fn new(
        action_editor: Option<FocusHandle>,
        keystrokes: FocusHandle,
        action_arguments: Option<FocusHandle>,
        context: FocusHandle,
    ) -> Self {
        Self {
            handles: Vec::from_iter(
                [
                    action_editor,
                    Some(keystrokes),
                    action_arguments,
                    Some(context),
                ]
                .into_iter()
                .flatten(),
            ),
        }
    }

    fn focused_index(&self, window: &Window, cx: &App) -> Option<i32> {
        self.handles
            .iter()
            .position(|handle| handle.contains_focused(window, cx))
            .map(|i| i as i32)
    }

    fn focus_index(&self, mut index: i32, window: &mut Window, cx: &mut App) {
        if index < 0 {
            index = self.handles.len() as i32 - 1;
        }
        if index >= self.handles.len() as i32 {
            index = 0;
        }
        window.focus(&self.handles[index as usize], cx);
    }

    fn focus_next(&self, window: &mut Window, cx: &mut App) {
        let index_to_focus = if let Some(index) = self.focused_index(window, cx) {
            index + 1
        } else {
            0
        };
        self.focus_index(index_to_focus, window, cx);
    }

    fn focus_previous(&self, window: &mut Window, cx: &mut App) {
        let index_to_focus = if let Some(index) = self.focused_index(window, cx) {
            index - 1
        } else {
            self.handles.len() as i32 - 1
        };
        self.focus_index(index_to_focus, window, cx);
    }
}
