use super::*;

impl Global for VimGlobals {}

impl VimGlobals {
    pub(crate) fn register(cx: &mut App) {
        cx.set_global(VimGlobals::default());

        cx.observe_keystrokes(|event, _, cx| {
            let Some(action) = event.action.as_ref().map(|action| action.boxed_clone()) else {
                return;
            };
            Vim::globals(cx).observe_action(action.boxed_clone())
        })
        .detach();

        cx.observe_new(|workspace: &mut Workspace, window, _| {
            RegistersView::register(workspace, window);
        })
        .detach();

        cx.observe_new(move |workspace: &mut Workspace, window, _| {
            MarksView::register(workspace, window);
        })
        .detach();

        let mut was_enabled = None;

        cx.observe_global::<SettingsStore>(move |cx| {
            let is_enabled = Vim::enabled(cx);
            if was_enabled == Some(is_enabled) {
                return;
            }
            was_enabled = Some(is_enabled);
            if is_enabled {
                KeyBinding::set_vim_mode(cx, true);
                CommandPaletteFilter::update_global(cx, |filter, _| {
                    filter.show_namespace(Vim::NAMESPACE);
                });
                GlobalCommandPaletteInterceptor::set(cx, command_interceptor);
                for window in cx.windows() {
                    if let Some(multi_workspace) = window.downcast::<MultiWorkspace>() {
                        multi_workspace
                            .update(cx, |multi_workspace, _, cx| {
                                for workspace in multi_workspace.workspaces() {
                                    workspace.update(cx, |workspace, cx| {
                                        Vim::update_globals(cx, |globals, cx| {
                                            globals.register_workspace(workspace, cx)
                                        });
                                    });
                                }
                            })
                            .ok();
                    }
                }
            } else {
                KeyBinding::set_vim_mode(cx, false);
                *Vim::globals(cx) = VimGlobals::default();
                GlobalCommandPaletteInterceptor::clear(cx);
                CommandPaletteFilter::update_global(cx, |filter, _| {
                    filter.hide_namespace(Vim::NAMESPACE);
                });
            }
        })
        .detach();
        cx.observe_new(|workspace: &mut Workspace, _, cx| {
            Vim::update_globals(cx, |globals, cx| globals.register_workspace(workspace, cx));
        })
        .detach()
    }

    fn register_workspace(&mut self, workspace: &Workspace, cx: &mut Context<Workspace>) {
        let entity_id = cx.entity_id();
        self.marks.insert(entity_id, MarksState::new(workspace, cx));
        cx.observe_release(&cx.entity(), move |_, _, cx| {
            Vim::update_globals(cx, |globals, _| {
                globals.marks.remove(&entity_id);
            })
        })
        .detach();
    }

    pub(crate) fn write_registers(
        &mut self,
        content: Register,
        register: Option<char>,
        is_yank: bool,
        kind: MotionKind,
        cx: &mut Context<Editor>,
    ) {
        if let Some(register) = register {
            let lower = register.to_lowercase().next().unwrap_or(register);
            if lower != register {
                let current = self.registers.entry(lower).or_default();
                current.text = (current.text.to_string() + &content.text).into();
                // not clear how to support appending to registers with multiple cursors
                current.clipboard_selections.take();
                let yanked = current.clone();
                self.registers.insert('"', yanked);
            } else {
                match lower {
                    '_' | ':' | '.' | '%' | '#' | '=' | '/' => {}
                    '+' => {
                        self.registers.insert('"', content.clone());
                        cx.write_to_clipboard(content.into());
                    }
                    '*' => {
                        self.registers.insert('"', content.clone());
                        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                        cx.write_to_primary(content.into());
                        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
                        cx.write_to_clipboard(content.into());
                    }
                    '"' => {
                        self.registers.insert('"', content.clone());
                        self.registers.insert('0', content);
                    }
                    _ => {
                        self.registers.insert('"', content.clone());
                        self.registers.insert(lower, content);
                    }
                }
            }
        } else {
            let setting = VimSettings::get_global(cx).use_system_clipboard;
            if setting == UseSystemClipboard::Always
                || setting == UseSystemClipboard::OnYank && is_yank
            {
                self.last_yank.replace(content.text.clone());
                cx.write_to_clipboard(content.clone().into());
            } else {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    self.last_yank.replace(text.into());
                }
            }
            self.registers.insert('"', content.clone());
            if is_yank {
                self.registers.insert('0', content);
            } else {
                let contains_newline = content.text.contains('\n');
                if !contains_newline {
                    self.registers.insert('-', content.clone());
                }
                if kind.linewise() || contains_newline {
                    let mut content = content;
                    for i in '1'..='9' {
                        if let Some(moved) = self.registers.insert(i, content) {
                            content = moved;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn read_register(
        &self,
        register: Option<char>,
        editor: Option<&mut Editor>,
        cx: &mut App,
    ) -> Option<Register> {
        let Some(register) = register.filter(|reg| *reg != '"') else {
            let setting = VimSettings::get_global(cx).use_system_clipboard;
            return match setting {
                UseSystemClipboard::Always => cx.read_from_clipboard().map(|item| item.into()),
                UseSystemClipboard::OnYank if self.system_clipboard_is_newer(cx) => {
                    cx.read_from_clipboard().map(|item| item.into())
                }
                _ => self.registers.get(&'"').cloned(),
            };
        };
        let lower = register.to_lowercase().next().unwrap_or(register);
        match lower {
            '_' | ':' | '.' | '#' | '=' => None,
            '+' => cx.read_from_clipboard().map(|item| item.into()),
            '*' => {
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                {
                    cx.read_from_primary().map(|item| item.into())
                }
                #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
                {
                    cx.read_from_clipboard().map(|item| item.into())
                }
            }
            '%' => editor.and_then(|editor| {
                let multibuffer = editor.buffer().read(cx);
                let snapshot = multibuffer.snapshot(cx);
                let selection = editor.selections.newest_anchor();
                let buffer = snapshot
                    .anchor_to_buffer_anchor(selection.head())
                    .and_then(|(text_anchor, _)| multibuffer.buffer(text_anchor.buffer_id));
                if let Some(buffer) = buffer {
                    buffer
                        .read(cx)
                        .file()
                        .map(|file| file.path().display(file.path_style(cx)).into_owned().into())
                } else {
                    None
                }
            }),
            _ => self.registers.get(&lower).cloned(),
        }
    }

    fn system_clipboard_is_newer(&self, cx: &App) -> bool {
        cx.read_from_clipboard().is_some_and(|item| {
            match (item.text().as_deref(), &self.last_yank) {
                (Some(new), Some(last)) => last.as_ref() != new,
                (Some(_), None) => true,
                (None, _) => false,
            }
        })
    }

    pub fn observe_action(&mut self, action: Box<dyn Action>) {
        if self.dot_recording {
            self.recording_actions
                .push(ReplayableAction::Action(action.boxed_clone()));

            if self.stop_recording_after_next_action {
                self.dot_recording = false;
                self.recorded_actions = std::mem::take(&mut self.recording_actions);
                self.recorded_count = self.recording_count.take();
                self.recorded_register_for_dot = self.recording_register_for_dot.take();
                self.stop_recording_after_next_action = false;
            }
        }
        if self.replayer.is_none()
            && let Some(recording_register) = self.recording_register
        {
            self.recordings
                .entry(recording_register)
                .or_default()
                .push(ReplayableAction::Action(action));
        }
    }

    pub fn observe_insertion(&mut self, text: &Arc<str>, range_to_replace: Option<Range<isize>>) {
        if self.ignore_current_insertion {
            self.ignore_current_insertion = false;
            return;
        }
        if self.dot_recording {
            self.recording_actions.push(ReplayableAction::Insertion {
                text: text.clone(),
                utf16_range_to_replace: range_to_replace.clone(),
            });
            if self.stop_recording_after_next_action {
                self.dot_recording = false;
                self.recorded_actions = std::mem::take(&mut self.recording_actions);
                self.recorded_count = self.recording_count.take();
                self.recorded_register_for_dot = self.recording_register_for_dot.take();
                self.stop_recording_after_next_action = false;
            }
        }
        if let Some(recording_register) = self.recording_register {
            self.recordings.entry(recording_register).or_default().push(
                ReplayableAction::Insertion {
                    text: text.clone(),
                    utf16_range_to_replace: range_to_replace,
                },
            );
        }
    }

    pub fn focused_vim(&self) -> Option<Entity<Vim>> {
        self.focused_vim.as_ref().and_then(|vim| vim.upgrade())
    }
}

impl Vim {
    pub fn globals(cx: &mut App) -> &mut VimGlobals {
        cx.global_mut::<VimGlobals>()
    }

    pub fn update_globals<C, R>(cx: &mut C, f: impl FnOnce(&mut VimGlobals, &mut C) -> R) -> R
    where
        C: BorrowMut<App>,
    {
        cx.update_global(f)
    }
}
