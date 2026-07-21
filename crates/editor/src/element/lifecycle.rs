use super::*;

pub struct EditorElement {
    pub(super) editor: Entity<Editor>,
    pub(super) style: EditorStyle,
    pub(super) split_side: Option<SplitSide>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitSide {
    Left,
    Right,
}

impl EditorElement {
    pub fn new(editor: &Entity<Editor>, style: EditorStyle) -> Self {
        Self {
            editor: editor.clone(),
            style,
            split_side: None,
        }
    }

    pub fn set_split_side(&mut self, side: SplitSide) {
        self.split_side = Some(side);
    }

    pub(super) fn register_key_listeners(
        &self,
        window: &mut Window,
        _: &mut App,
        layout: &EditorLayout,
    ) {
        let position_map = layout.position_map.clone();
        window.on_key_event({
            let editor = self.editor.clone();
            move |event: &ModifiersChangedEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                editor.update(cx, |editor, cx| {
                    let inlay_hint_settings = inlay_hint_settings(
                        editor.selections.newest_anchor().head(),
                        &editor.buffer.read(cx).snapshot(cx),
                        cx,
                    );

                    if let Some(inlay_modifiers) = inlay_hint_settings
                        .toggle_on_modifiers_press
                        .as_ref()
                        .filter(|modifiers| modifiers.modified())
                    {
                        editor.refresh_inlay_hints(
                            InlayHintRefreshReason::ModifiersChanged(
                                inlay_modifiers == &event.modifiers,
                            ),
                            cx,
                        );
                    }

                    if editor.hover_state.focused(window, cx) {
                        return;
                    }

                    editor.handle_modifiers_changed(event.modifiers, &position_map, window, cx);
                })
            }
        });
    }

    pub(super) fn editor_with_selections(&self, cx: &App) -> Option<Entity<Editor>> {
        if let EditorMode::Minimap { parent } = self.editor.read(cx).mode() {
            parent.upgrade()
        } else {
            Some(self.editor.clone())
        }
    }
}
