use super::*;

impl Pane {
    pub fn handle_item_edit(&mut self, item_id: EntityId, cx: &App) {
        if let Some(preview_item) = self.preview_item()
            && preview_item.item_id() == item_id
            && !preview_item.preserve_preview(cx)
        {
            self.unpreview_item_if_preview(item_id);
        }
    }

    pub(crate) fn open_item(
        &mut self,
        project_entry_id: Option<ProjectEntryId>,
        project_path: ProjectPath,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        suggested_position: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build_item: WorkspaceItemBuilder,
    ) -> Box<dyn ItemHandle> {
        let mut existing_item = None;
        if let Some(project_entry_id) = project_entry_id {
            for (index, item) in self.items.iter().enumerate() {
                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                    && item.project_entry_ids(cx).as_slice() == [project_entry_id]
                {
                    let item = item.boxed_clone();
                    existing_item = Some((index, item));
                    break;
                }
            }
        } else {
            for (index, item) in self.items.iter().enumerate() {
                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                    && item.project_path(cx).as_ref() == Some(&project_path)
                {
                    let item = item.boxed_clone();
                    existing_item = Some((index, item));
                    break;
                }
            }
        }

        let preview_was_active = self.preview_item_idx() == Some(self.active_item_index);

        let set_up_existing_item =
            |index: usize, pane: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                if !allow_preview && let Some(item) = pane.items.get(index) {
                    pane.unpreview_item_if_preview(item.item_id());
                }
                if activate {
                    pane.activate_item(index, focus_item, focus_item, window, cx);
                }
            };
        let set_up_new_item = |new_item: Box<dyn ItemHandle>,
                               destination_index: Option<usize>,
                               pane: &mut Self,
                               window: &mut Window,
                               cx: &mut Context<Self>| {
            let new_item_id = new_item.item_id();

            if allow_preview && preview_was_active {
                pane.set_preview_item_id(Some(new_item_id), cx);
            }

            if let Some(text) = new_item.telemetry_event_text(cx) {
                telemetry::event!(text);
            }

            pane.add_item_inner(
                new_item,
                true,
                focus_item,
                activate,
                destination_index,
                window,
                cx,
            );

            if allow_preview && !preview_was_active {
                pane.set_preview_item_id(Some(new_item_id), cx);
            }
        };

        if let Some((index, existing_item)) = existing_item {
            set_up_existing_item(index, self, window, cx);
            existing_item
        } else {
            // If the item is being opened as preview and we have an existing preview tab,
            // open the new item in the position of the existing preview tab.
            let destination_index = if allow_preview {
                self.close_current_preview_item(window, cx)
            } else {
                suggested_position
            };

            let new_item = build_item(self, window, cx);
            // A special case that won't ever get a `project_entry_id` but has to be deduplicated nonetheless.
            if let Some(invalid_buffer_view) = new_item.downcast::<InvalidItemView>() {
                let mut already_open_view = None;
                let mut views_to_close = HashSet::default();
                for existing_error_view in self
                    .items_of_type::<InvalidItemView>()
                    .filter(|item| item.read(cx).abs_path == invalid_buffer_view.read(cx).abs_path)
                {
                    if already_open_view.is_none()
                        && existing_error_view.read(cx).error == invalid_buffer_view.read(cx).error
                    {
                        already_open_view = Some(existing_error_view);
                    } else {
                        views_to_close.insert(existing_error_view.item_id());
                    }
                }

                let resulting_item = match already_open_view {
                    Some(already_open_view) => {
                        if let Some(index) = self.index_for_item_id(already_open_view.item_id()) {
                            set_up_existing_item(index, self, window, cx);
                        }
                        Box::new(already_open_view) as Box<_>
                    }
                    None => {
                        set_up_new_item(new_item.clone(), destination_index, self, window, cx);
                        new_item
                    }
                };

                self.close_items(window, cx, SaveIntent::Skip, &|existing_item| {
                    views_to_close.contains(&existing_item)
                })
                .detach();

                resulting_item
            } else {
                set_up_new_item(new_item.clone(), destination_index, self, window, cx);
                new_item
            }
        }
    }

    pub fn close_current_preview_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let item_idx = self.preview_item_idx()?;
        let id = self.preview_item_id()?;
        self.preview_item_id = None;

        let prev_active_item_index = self.active_item_index;
        self.remove_item(id, false, false, window, cx);
        self.active_item_index = prev_active_item_index;
        if item_idx < prev_active_item_index {
            self.active_item_index -= 1;
        }
        self.nav_history.0.lock().preview_item_id = None;

        if item_idx < self.items.len() {
            Some(item_idx)
        } else {
            None
        }
    }

    pub fn add_item_inner(
        &mut self,
        item: Box<dyn ItemHandle>,
        activate_pane: bool,
        focus_item: bool,
        activate: bool,
        destination_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let item_already_exists = self
            .items
            .iter()
            .any(|existing_item| existing_item.item_id() == item.item_id());

        if !item_already_exists {
            self.close_items_on_item_open(window, cx);
        }

        if item.buffer_kind(cx) == ItemBufferKind::Singleton
            && let Some(&entry_id) = item.project_entry_ids(cx).first()
        {
            let Some(project) = self.project.upgrade() else {
                return;
            };

            let project = project.read(cx);
            if let Some(project_path) = project.path_for_entry(entry_id, cx) {
                let abs_path = project.absolute_path(&project_path, cx);
                self.nav_history
                    .0
                    .lock()
                    .paths_by_item
                    .insert(item.item_id(), (project_path, abs_path));
            }
        }
        // If no destination index is specified, add or move the item after the
        // active item (or at the start of tab bar, if the active item is pinned)
        let mut insertion_index = {
            cmp::min(
                if let Some(destination_index) = destination_index {
                    destination_index
                } else {
                    cmp::max(self.active_item_index + 1, self.pinned_count())
                },
                self.items.len(),
            )
        };

        // Does the item already exist?
        let project_entry_id = if item.buffer_kind(cx) == ItemBufferKind::Singleton {
            item.project_entry_ids(cx).first().copied()
        } else {
            None
        };

        let existing_item_index = self.items.iter().position(|existing_item| {
            if existing_item.item_id() == item.item_id() {
                true
            } else if existing_item.buffer_kind(cx) == ItemBufferKind::Singleton {
                existing_item
                    .project_entry_ids(cx)
                    .first()
                    .is_some_and(|existing_entry_id| {
                        Some(existing_entry_id) == project_entry_id.as_ref()
                    })
            } else {
                false
            }
        });
        if let Some(existing_item_index) = existing_item_index {
            // If the item already exists, move it to the desired destination and activate it

            if existing_item_index != insertion_index {
                let existing_item_is_active = existing_item_index == self.active_item_index;

                // If the caller didn't specify a destination and the added item is already
                // the active one, don't move it
                if existing_item_is_active && destination_index.is_none() {
                    insertion_index = existing_item_index;
                } else {
                    self.items.remove(existing_item_index);
                    if existing_item_index < self.active_item_index {
                        self.active_item_index -= 1;
                    }
                    insertion_index = insertion_index.min(self.items.len());

                    self.items.insert(insertion_index, item.clone());

                    if existing_item_is_active {
                        self.active_item_index = insertion_index;
                    } else if insertion_index <= self.active_item_index {
                        self.active_item_index += 1;
                    }
                }

                cx.notify();
            }

            if activate {
                self.activate_item(insertion_index, activate_pane, focus_item, window, cx);
            }
        } else {
            self.items.insert(insertion_index, item.clone());
            cx.notify();

            if activate {
                if insertion_index <= self.active_item_index
                    && self.preview_item_idx() != Some(self.active_item_index)
                {
                    self.active_item_index += 1;
                }

                self.activate_item(insertion_index, activate_pane, focus_item, window, cx);
            }
        }

        cx.emit(Event::AddItem { item });
    }

    pub fn add_item(
        &mut self,
        item: Box<dyn ItemHandle>,
        activate_pane: bool,
        focus_item: bool,
        destination_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = item.telemetry_event_text(cx) {
            telemetry::event!(text);
        }

        self.add_item_inner(
            item,
            activate_pane,
            focus_item,
            true,
            destination_index,
            window,
            cx,
        )
    }
}
