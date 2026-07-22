use super::*;

pub(super) fn register_setup(editor: &mut Editor, cx: &mut Context<Vim>) {
    Vim::action(editor, cx, |vim, action: &VimSet, _, cx| {
        for option in action.options.iter() {
            vim.update_editor(cx, |_, editor, cx| match option {
                VimOption::Wrap(true) => {
                    editor
                        .set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
                }
                VimOption::Wrap(false) => {
                    editor.set_soft_wrap_mode(language::language_settings::SoftWrap::None, cx);
                }
                VimOption::Number(enabled) => {
                    editor.set_show_line_numbers(*enabled, cx);
                }
                VimOption::RelativeNumber(enabled) => {
                    editor.set_relative_line_number(Some(*enabled), cx);
                }
                VimOption::IgnoreCase(enabled) => {
                    let mut settings = EditorSettings::get_global(cx).clone();
                    settings.search.case_sensitive = !*enabled;
                    SettingsStore::update(cx, |store, _| {
                        store.override_global(settings);
                    });
                }
                VimOption::GDefault(enabled) => {
                    let mut settings = VimSettings::get_global(cx).clone();
                    settings.gdefault = *enabled;

                    SettingsStore::update(cx, |store, _| {
                        store.override_global(settings);
                    })
                }
            });
        }
    });
    Vim::action(editor, cx, |vim, _: &VisualCommand, window, cx| {
        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            command_palette::CommandPalette::toggle(workspace, "'<,'>", window, cx);
        })
    });

    Vim::action(editor, cx, |vim, _: &ShellCommand, window, cx| {
        let Some(workspace) = vim.workspace(window, cx) else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            command_palette::CommandPalette::toggle(workspace, "'<,'>!", window, cx);
        })
    });

    Vim::action(editor, cx, |_, _: &ArgumentRequired, window, cx| {
        let _ = window.prompt(
            gpui::PromptLevel::Critical,
            "Argument required",
            None,
            &["Cancel"],
            cx,
        );
    });
}
