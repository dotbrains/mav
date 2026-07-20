use super::*;

pub fn register_action<T: Action>(
    editor: &Entity<Editor>,
    window: &mut Window,
    listener: impl Fn(&mut Editor, &T, &mut Window, &mut Context<Editor>) + 'static,
) {
    let editor = editor.clone();
    window.on_action(TypeId::of::<T>(), move |action, phase, window, cx| {
        let action = action.downcast_ref().unwrap();
        if phase == DispatchPhase::Bubble {
            editor.update(cx, |editor, cx| {
                listener(editor, action, window, cx);
            })
        }
    })
}
