use super::*;

pub fn register(editor: &mut Editor, cx: &mut Context<Vim>) {
    register_setup::register_setup(editor, cx);
    register_save::register_save(editor, cx);
    register_file_commands::register_file_commands(editor, cx);
    register_normal_commands::register_normal_commands(editor, cx);
    register_range_commands::register_range_commands(editor, cx);
}
