use super::*;

impl App {
    /// Run `f` against the entity's *current* window — the most recently
    /// rendered window that referenced the entity, or its creation window if
    /// it has yet to be rendered. Returns `None` if the entity has no
    /// current window, or if that window has been closed, or if it is
    /// already on the update stack.
    pub fn with_window<R>(
        &mut self,
        entity_id: EntityId,
        f: impl FnOnce(&mut Window, &mut App) -> R,
    ) -> Option<R> {
        let window_id = *self.current_window_by_entity.get(&entity_id)?;
        self.update_window_id(window_id, |_, window, cx| f(window, cx))
            .ok()
    }
    pub(crate) fn ensure_window(&mut self, entity_id: EntityId, window: WindowId) {
        self.current_window_by_entity
            .entry(entity_id)
            .or_insert(window);
    }

    pub(crate) fn update_window_id<T, F>(&mut self, id: WindowId, update: F) -> Result<T>
    where
        F: FnOnce(AnyView, &mut Window, &mut App) -> T,
    {
        self.update(|cx| {
            let mut window = cx.windows.get_mut(id)?.take()?;

            let root_view = window.root.clone().unwrap();

            cx.window_update_stack.push(window.handle.id);
            let result = update(root_view, &mut window, cx);
            fn trail(id: WindowId, window: Box<Window>, cx: &mut App) -> Option<()> {
                cx.window_update_stack.pop();

                if window.removed {
                    cx.window_handles.remove(&id);
                    cx.windows.remove(id);
                    if let Some(tracked) = cx.tracked_entities.remove(&id) {
                        for entity_id in tracked {
                            if let Some(windows) =
                                cx.window_invalidators_by_entity.get_mut(&entity_id)
                            {
                                windows.remove(&id);
                            }
                            if cx.current_window_by_entity.get(&entity_id) == Some(&id) {
                                cx.current_window_by_entity.remove(&entity_id);
                            }
                        }
                    }

                    cx.window_closed_observers.clone().retain(&(), |callback| {
                        callback(cx, id);
                        true
                    });

                    let quit_on_empty = match cx.quit_mode {
                        QuitMode::Explicit => false,
                        QuitMode::LastWindowClosed => true,
                        QuitMode::Default => cfg!(not(target_os = "macos")),
                    };

                    if quit_on_empty && cx.windows.is_empty() {
                        cx.quit();
                    }
                } else {
                    cx.windows.get_mut(id)?.replace(window);
                }
                Some(())
            }
            trail(id, window, cx)?;

            Some(result)
        })
        .context("window not found")
    }
}
