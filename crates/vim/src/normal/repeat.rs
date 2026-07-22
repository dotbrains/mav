use std::{cell::RefCell, rc::Rc};

use crate::{
    Vim,
    insert::NormalBefore,
    motion::Motion,
    normal::InsertBefore,
    state::{Mode, Operator, RecordedSelection, ReplayableAction, VimGlobals},
};
use editor::Editor;
use gpui::{Action, App, Context, Window, actions};
use workspace::Workspace;

actions!(
    vim,
    [
        /// Repeats the last change.
        Repeat,
        /// Ends the repeat recording.
        EndRepeat,
        /// Toggles macro recording.
        ToggleRecord,
        /// Replays the last recorded macro.
        ReplayLastRecording
    ]
);

fn should_replay(action: &dyn Action) -> bool {
    // skip so that we don't leave the character palette open
    if editor::actions::ShowCharacterPalette.partial_eq(action) {
        return false;
    }
    true
}

fn repeatable_insert(action: &ReplayableAction) -> Option<Box<dyn Action>> {
    match action {
        ReplayableAction::Action(action) => {
            if super::InsertBefore.partial_eq(&**action)
                || super::InsertAfter.partial_eq(&**action)
                || super::InsertFirstNonWhitespace.partial_eq(&**action)
                || super::InsertEndOfLine.partial_eq(&**action)
            {
                Some(super::InsertBefore.boxed_clone())
            } else if super::InsertLineAbove.partial_eq(&**action)
                || super::InsertLineBelow.partial_eq(&**action)
            {
                Some(super::InsertLineBelow.boxed_clone())
            } else if crate::replace::ToggleReplace.partial_eq(&**action) {
                Some(crate::replace::ToggleReplace.boxed_clone())
            } else {
                None
            }
        }
        ReplayableAction::Insertion { .. } => None,
    }
}

pub(crate) fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, _: &EndRepeat, window, cx| {
        Vim::globals(cx).dot_replaying = false;
        vim.switch_mode(Mode::Normal, false, window, cx)
    });

    Vim::action(editor, cx, |vim, _: &Repeat, window, cx| {
        vim.repeat(false, window, cx)
    });

    Vim::action(editor, cx, |vim, _: &ToggleRecord, window, cx| {
        let globals = Vim::globals(cx);
        if let Some(char) = globals.recording_register.take() {
            globals.last_recorded_register = Some(char)
        } else {
            vim.push_operator(Operator::RecordRegister, window, cx);
        }
    });

    Vim::action(editor, cx, |vim, _: &ReplayLastRecording, window, cx| {
        let Some(register) = Vim::globals(cx).last_recorded_register else {
            return;
        };
        vim.replay_register(register, window, cx)
    });
}

pub struct ReplayerState {
    actions: Vec<ReplayableAction>,
    running: bool,
    ix: usize,
}

#[derive(Clone)]
pub struct Replayer(Rc<RefCell<ReplayerState>>);

impl Replayer {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(ReplayerState {
            actions: vec![],
            running: false,
            ix: 0,
        })))
    }

    pub fn replay(&mut self, actions: Vec<ReplayableAction>, window: &mut Window, cx: &mut App) {
        let mut lock = self.0.borrow_mut();
        let range = lock.ix..lock.ix;
        lock.actions.splice(range, actions);
        if lock.running {
            return;
        }
        lock.running = true;
        let this = self.clone();
        window.defer(cx, move |window, cx| {
            this.next(window, cx);
            let Some(workspace) = Workspace::for_window(window, cx) else {
                return;
            };
            let Some(editor) = workspace
                .read(cx)
                .active_item(cx)
                .and_then(|item| item.act_as::<Editor>(cx))
            else {
                return;
            };
            editor.update(cx, |editor, cx| {
                editor
                    .buffer()
                    .update(cx, |multi, cx| multi.finalize_last_transaction(cx))
            });
        })
    }

    pub fn stop(self) {
        self.0.borrow_mut().actions.clear()
    }

    pub fn next(self, window: &mut Window, cx: &mut App) {
        let mut lock = self.0.borrow_mut();
        let action = if lock.ix < 10000 {
            lock.actions.get(lock.ix).cloned()
        } else {
            log::error!("Aborting replay after 10000 actions");
            None
        };
        lock.ix += 1;
        drop(lock);
        let Some(action) = action else {
            // The `globals.dot_replaying = false` is a fail-safe to ensure that
            // this value is always reset, in the case that the focus is moved
            // away from the editor, effectively preventing the `EndRepeat`
            // action from being handled.
            let globals = Vim::globals(cx);
            globals.replayer.take();
            globals.dot_replaying = false;
            return;
        };
        match action {
            ReplayableAction::Action(action) => {
                if should_replay(&*action) {
                    window.dispatch_action(action.boxed_clone(), cx);
                    cx.defer(move |cx| Vim::globals(cx).observe_action(action.boxed_clone()));
                }
            }
            ReplayableAction::Insertion {
                text,
                utf16_range_to_replace,
            } => {
                let Some(workspace) = Workspace::for_window(window, cx) else {
                    return;
                };
                let Some(editor) = workspace
                    .read(cx)
                    .active_item(cx)
                    .and_then(|item| item.act_as::<Editor>(cx))
                else {
                    return;
                };
                editor.update(cx, |editor, cx| {
                    editor.replay_insert_event(&text, utf16_range_to_replace.clone(), window, cx)
                })
            }
        }
        window.defer(cx, move |window, cx| self.next(window, cx));
    }
}

impl Vim {
    pub(crate) fn record_register(
        &mut self,
        register: char,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let globals = Vim::globals(cx);
        globals.recording_register = Some(register);
        globals.recordings.remove(&register);
        globals.ignore_current_insertion = true;
        self.clear_operator(window, cx)
    }

    pub(crate) fn replay_register(
        &mut self,
        mut register: char,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut count = Vim::take_count(cx).unwrap_or(1);
        Vim::take_forced_motion(cx);
        self.clear_operator(window, cx);

        let globals = Vim::globals(cx);
        if register == '@' {
            let Some(last) = globals.last_replayed_register else {
                return;
            };
            register = last;
        }
        let Some(actions) = globals.recordings.get(&register) else {
            return;
        };

        let mut repeated_actions = vec![];
        while count > 0 {
            repeated_actions.extend(actions.iter().cloned());
            count -= 1
        }

        globals.last_replayed_register = Some(register);
        let mut replayer = globals.replayer.get_or_insert_with(Replayer::new).clone();
        replayer.replay(repeated_actions, window, cx);
    }

    pub(crate) fn repeat(
        &mut self,
        from_insert_mode: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_operator().is_some() {
            Vim::update_globals(cx, |globals, _| {
                globals.recording_actions.clear();
                globals.recording_count = None;
                globals.dot_recording = false;
                globals.stop_recording_after_next_action = false;
            });
            self.clear_operator(window, cx);
            return;
        }

        Vim::take_forced_motion(cx);
        let count = Vim::take_count(cx);

        let Some((mut actions, selection, mode)) = Vim::update_globals(cx, |globals, _| {
            let actions = globals.recorded_actions.clone();
            if actions.is_empty() {
                return None;
            }
            if globals.replayer.is_none()
                && let Some(recording_register) = globals.recording_register
            {
                globals
                    .recordings
                    .entry(recording_register)
                    .or_default()
                    .push(ReplayableAction::Action(Repeat.boxed_clone()));
            }

            let mut mode = None;
            let selection = globals.recorded_selection.clone();
            match selection {
                RecordedSelection::SingleLine { .. } | RecordedSelection::Visual { .. } => {
                    globals.recorded_count = None;
                    mode = Some(Mode::Visual);
                }
                RecordedSelection::VisualLine { .. } => {
                    globals.recorded_count = None;
                    mode = Some(Mode::VisualLine)
                }
                RecordedSelection::VisualBlock { .. } => {
                    globals.recorded_count = None;
                    mode = Some(Mode::VisualBlock)
                }
                RecordedSelection::None => {
                    if let Some(count) = count {
                        globals.recorded_count = Some(count);
                    }
                }
            }

            Some((actions, selection, mode))
        }) else {
            return;
        };

        // Dot repeat always uses the recorded register, ignoring any "X
        // override, as the register is an inherent part of the recorded action.
        // For numbered registers, Neovim increments on each dot repeat so after
        // using `"1p`, using `.` will equate to `"2p", the next `.` to `"3p`,
        // etc..
        let recorded_register = cx.global::<VimGlobals>().recorded_register_for_dot;
        let next_register = recorded_register
            .filter(|c| matches!(c, '1'..='9'))
            .map(|c| ((c as u8 + 1).min(b'9')) as char);

        self.selected_register = next_register.or(recorded_register);
        if let Some(next_register) = next_register {
            Vim::update_globals(cx, |globals, _| {
                globals.recorded_register_for_dot = Some(next_register)
            })
        };

        if mode != Some(self.mode) {
            if let Some(mode) = mode {
                self.switch_mode(mode, false, window, cx)
            }

            match selection {
                RecordedSelection::SingleLine { cols } => {
                    if cols > 1 {
                        self.visual_motion(Motion::Right, Some(cols as usize - 1), window, cx)
                    }
                }
                RecordedSelection::Visual { rows, cols } => {
                    self.visual_motion(
                        Motion::Down {
                            display_lines: false,
                        },
                        Some(rows as usize),
                        window,
                        cx,
                    );
                    self.visual_motion(
                        Motion::StartOfLine {
                            display_lines: false,
                        },
                        None,
                        window,
                        cx,
                    );
                    if cols > 1 {
                        self.visual_motion(Motion::Right, Some(cols as usize - 1), window, cx)
                    }
                }
                RecordedSelection::VisualBlock { rows, cols } => {
                    self.visual_motion(
                        Motion::Down {
                            display_lines: false,
                        },
                        Some(rows as usize),
                        window,
                        cx,
                    );
                    if cols > 1 {
                        self.visual_motion(Motion::Right, Some(cols as usize - 1), window, cx);
                    }
                }
                RecordedSelection::VisualLine { rows } => {
                    self.visual_motion(
                        Motion::Down {
                            display_lines: false,
                        },
                        Some(rows as usize),
                        window,
                        cx,
                    );
                }
                RecordedSelection::None => {}
            }
        }

        // insert internally uses repeat to handle counts
        // vim doesn't treat 3a1 as though you literally repeated a1
        // 3 times, instead it inserts the content thrice at the insert position.
        if let Some(to_repeat) = repeatable_insert(&actions[0]) {
            if let Some(ReplayableAction::Action(action)) = actions.last()
                && NormalBefore.partial_eq(&**action)
            {
                actions.pop();
            }

            let mut new_actions = actions.clone();
            actions[0] = ReplayableAction::Action(to_repeat.boxed_clone());

            let mut count = cx.global::<VimGlobals>().recorded_count.unwrap_or(1);

            // if we came from insert mode we're just doing repetitions 2 onwards.
            if from_insert_mode {
                count -= 1;
                new_actions[0] = actions[0].clone();
            }

            for _ in 1..count {
                new_actions.append(actions.clone().as_mut());
            }
            new_actions.push(ReplayableAction::Action(NormalBefore.boxed_clone()));
            actions = new_actions;
        }

        actions.push(ReplayableAction::Action(EndRepeat.boxed_clone()));

        if self.temp_mode {
            self.temp_mode = false;
            actions.push(ReplayableAction::Action(InsertBefore.boxed_clone()));
        }

        let globals = Vim::globals(cx);
        globals.dot_replaying = true;
        let mut replayer = globals.replayer.get_or_insert_with(Replayer::new).clone();

        replayer.replay(actions, window, cx);
    }
}

#[cfg(test)]
mod test;
