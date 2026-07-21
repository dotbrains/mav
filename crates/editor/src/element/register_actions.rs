use super::action_registration::register_action;
use super::*;

impl EditorElement {
    pub(super) fn register_actions(&self, window: &mut Window, cx: &mut App) {
        let editor = &self.editor;
        editor.update(cx, |editor, cx| {
            for action in editor.editor_actions.borrow().values() {
                (action)(editor, window, cx)
            }
        });

        crate::rust_analyzer_ext::apply_related_actions(editor, window, cx);
        crate::clangd_ext::apply_related_actions(editor, window, cx);

        register_action(editor, window, Editor::open_context_menu);
        register_action(editor, window, Editor::move_left);
        register_action(editor, window, Editor::move_right);
        register_action(editor, window, Editor::move_down);
        register_action(editor, window, Editor::move_down_by_lines);
        register_action(editor, window, Editor::select_down_by_lines);
        register_action(editor, window, Editor::move_up);
        register_action(editor, window, Editor::move_up_by_lines);
        register_action(editor, window, Editor::select_up_by_lines);
        register_action(editor, window, Editor::select_page_down);
        register_action(editor, window, Editor::select_page_up);
        register_action(editor, window, Editor::cancel);
        register_action(editor, window, Editor::blame_hover);
        register_action(editor, window, Editor::next_snippet_tabstop);
        register_action(editor, window, Editor::previous_snippet_tabstop);
        register_action(editor, window, Editor::copy);
        register_action(editor, window, Editor::copy_and_trim);
        register_action(editor, window, Editor::diff_clipboard_with_selection);
        register_action(editor, window, Editor::move_page_up);
        register_action(editor, window, Editor::move_page_down);
        register_action(editor, window, Editor::next_screen);
        register_action(editor, window, Editor::scroll_cursor_top);
        register_action(editor, window, Editor::scroll_cursor_center);
        register_action(editor, window, Editor::scroll_cursor_bottom);
        register_action(editor, window, Editor::scroll_cursor_center_top_bottom);
        register_action(editor, window, |editor, _: &LineDown, window, cx| {
            editor.scroll_screen(&ScrollAmount::Line(1.), window, cx)
        });
        register_action(editor, window, |editor, _: &LineUp, window, cx| {
            editor.scroll_screen(&ScrollAmount::Line(-1.), window, cx)
        });
        register_action(editor, window, |editor, _: &HalfPageDown, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(0.5), window, cx)
        });
        register_action(editor, window, |editor, _: &HalfPageUp, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(-0.5), window, cx)
        });
        register_action(editor, window, |editor, _: &PageDown, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(1.), window, cx)
        });
        register_action(editor, window, |editor, _: &PageUp, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(-1.), window, cx)
        });
        register_action(editor, window, Editor::move_to_previous_word_start);
        register_action(editor, window, Editor::move_to_previous_subword_start);
        register_action(editor, window, Editor::move_to_next_word_end);
        register_action(editor, window, Editor::move_to_next_subword_end);
        register_action(editor, window, Editor::move_to_beginning_of_line);
        register_action(editor, window, Editor::move_to_end_of_line);
        register_action(editor, window, Editor::move_to_start_of_paragraph);
        register_action(editor, window, Editor::move_to_end_of_paragraph);
        register_action(editor, window, Editor::move_to_beginning);
        register_action(editor, window, Editor::move_to_end);
        register_action(editor, window, Editor::move_to_start_of_excerpt);
        register_action(editor, window, Editor::move_to_start_of_next_excerpt);
        register_action(editor, window, Editor::move_to_end_of_excerpt);
        register_action(editor, window, Editor::move_to_end_of_previous_excerpt);
        register_action(editor, window, Editor::select_up);
        register_action(editor, window, Editor::select_down);
        register_action(editor, window, Editor::select_left);
        register_action(editor, window, Editor::select_right);
        register_action(editor, window, Editor::select_to_previous_word_start);
        register_action(editor, window, Editor::select_to_previous_subword_start);
        register_action(editor, window, Editor::select_to_next_word_end);
        register_action(editor, window, Editor::select_to_next_subword_end);
        register_action(editor, window, Editor::select_to_beginning_of_line);
        register_action(editor, window, Editor::select_to_end_of_line);
        register_action(editor, window, Editor::select_to_start_of_paragraph);
        register_action(editor, window, Editor::select_to_end_of_paragraph);
        register_action(editor, window, Editor::select_to_start_of_excerpt);
        register_action(editor, window, Editor::select_to_start_of_next_excerpt);
        register_action(editor, window, Editor::select_to_end_of_excerpt);
        register_action(editor, window, Editor::select_to_end_of_previous_excerpt);
        register_action(editor, window, Editor::select_to_beginning);
        register_action(editor, window, Editor::select_to_end);
        register_action(editor, window, Editor::select_all);
        register_action(editor, window, |editor, action, window, cx| {
            editor.select_all_matches(action, window, cx).log_err();
        });
        register_action(editor, window, Editor::select_line);
        register_action(editor, window, Editor::split_selection_into_lines);
        register_action(editor, window, Editor::add_selection_above);
        register_action(editor, window, Editor::add_selection_below);
        register_action(editor, window, Editor::insert_snippet_at_selections);
        register_action(editor, window, |editor, action, window, cx| {
            editor.select_next(action, window, cx).log_err();
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor.select_previous(action, window, cx).log_err();
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor.find_next_match(action, window, cx).log_err();
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor.find_previous_match(action, window, cx).log_err();
        });
        register_action(editor, window, Editor::select_larger_syntax_node);
        register_action(editor, window, Editor::select_smaller_syntax_node);
        register_action(editor, window, Editor::select_next_syntax_node);
        register_action(editor, window, Editor::select_prev_syntax_node);
        register_action(
            editor,
            window,
            Editor::select_to_start_of_larger_syntax_node,
        );
        register_action(editor, window, Editor::select_to_end_of_larger_syntax_node);
        register_action(editor, window, Editor::move_to_start_of_larger_syntax_node);
        register_action(editor, window, Editor::move_to_end_of_larger_syntax_node);
        register_action(editor, window, Editor::select_enclosing_symbol);
        register_action(editor, window, Editor::move_to_enclosing_bracket);
        register_action(editor, window, Editor::select_inside_delimiters);
        register_action(editor, window, Editor::select_around_delimiters);
        register_action(editor, window, Editor::undo_selection);
        register_action(editor, window, Editor::redo_selection);
        if editor.read(cx).buffer_kind(cx) == ItemBufferKind::Multibuffer {
            register_action(editor, window, Editor::expand_excerpts);
            register_action(editor, window, Editor::expand_excerpts_up);
            register_action(editor, window, Editor::expand_excerpts_down);
        }
        register_action(editor, window, Editor::go_to_diagnostic);
        register_action(editor, window, Editor::go_to_prev_diagnostic);
        register_action(editor, window, Editor::go_to_next_hunk);
        register_action(editor, window, Editor::go_to_prev_hunk);
        register_action(editor, window, Editor::go_to_next_document_highlight);
        register_action(editor, window, Editor::go_to_prev_document_highlight);
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_definition(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_definition_split(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_declaration(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_declaration_split(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_implementation(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_implementation_split(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_type_definition(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, |editor, action, window, cx| {
            editor
                .go_to_type_definition_split(action, window, cx)
                .detach_and_log_err(cx);
        });
        register_action(editor, window, Editor::open_url);
        register_action(editor, window, Editor::open_selected_filename);
        register_action(editor, window, Editor::fold);
        register_action(editor, window, Editor::fold_at_level);
        register_action(editor, window, Editor::fold_at_level_1);
        register_action(editor, window, Editor::fold_at_level_2);
        register_action(editor, window, Editor::fold_at_level_3);
        register_action(editor, window, Editor::fold_at_level_4);
        register_action(editor, window, Editor::fold_at_level_5);
        register_action(editor, window, Editor::fold_at_level_6);
        register_action(editor, window, Editor::fold_at_level_7);
        register_action(editor, window, Editor::fold_at_level_8);
        register_action(editor, window, Editor::fold_at_level_9);
        register_action(editor, window, Editor::fold_all);
        register_action(editor, window, Editor::fold_function_bodies);
        register_action(editor, window, Editor::fold_recursive);
        register_action(editor, window, Editor::toggle_fold);
        register_action(editor, window, Editor::toggle_fold_recursive);
        register_action(editor, window, Editor::toggle_fold_all);
        register_action(editor, window, Editor::unfold_lines);
        register_action(editor, window, Editor::unfold_recursive);
        register_action(editor, window, Editor::unfold_all);
        register_action(editor, window, Editor::fold_selected_ranges);
        register_action(editor, window, Editor::set_mark);
        register_action(editor, window, Editor::save_location);
        register_action(editor, window, Editor::swap_selection_ends);
        register_action(editor, window, Editor::show_completions);
        register_action(editor, window, Editor::show_word_completions);
        register_action(editor, window, Editor::toggle_code_actions);
        register_action(editor, window, Editor::open_excerpts);
        register_action(editor, window, Editor::open_excerpts_in_split);
        register_action(editor, window, Editor::toggle_soft_wrap);
        register_action(editor, window, Editor::toggle_tab_bar);
        register_action(editor, window, Editor::toggle_breadcrumb);
        register_action(editor, window, Editor::toggle_line_numbers);
        register_action(editor, window, Editor::toggle_relative_line_numbers);
        register_action(editor, window, Editor::toggle_indent_guides);
        register_action(editor, window, Editor::toggle_inlay_hints);
        register_action(editor, window, Editor::toggle_inline_values);
        register_action(editor, window, Editor::toggle_code_lens_action);
        register_action(editor, window, Editor::toggle_semantic_highlights);
        register_action(editor, window, Editor::toggle_edit_predictions);
        if editor.read(cx).diagnostics_enabled() {
            register_action(editor, window, Editor::toggle_diagnostics);
        }
        if editor.read(cx).inline_diagnostics_enabled() {
            register_action(editor, window, Editor::toggle_inline_diagnostics);
        }
        if editor.read(cx).supports_minimap(cx) {
            register_action(editor, window, Editor::toggle_minimap);
        }
        register_action(editor, window, hover_popover::hover);
        register_action(editor, window, Editor::reveal_in_finder);
        register_action(editor, window, Editor::copy_path);
        register_action(editor, window, Editor::copy_relative_path);
        register_action(editor, window, Editor::copy_file_name);
        register_action(editor, window, Editor::copy_file_name_without_extension);
        register_action(editor, window, Editor::copy_highlight_json);
        register_action(editor, window, Editor::copy_permalink_to_line);
        register_action(editor, window, Editor::open_permalink_to_line);
        register_action(editor, window, Editor::copy_file_location);
        register_action(editor, window, Editor::toggle_git_blame);
        register_action(editor, window, Editor::toggle_git_blame_inline);
        register_action(editor, window, Editor::open_git_blame_commit);
        register_action(editor, window, Editor::toggle_selected_diff_hunks);
        register_action(editor, window, Editor::toggle_staged_selected_diff_hunks);
        register_action(editor, window, Editor::stage_and_next);
        register_action(editor, window, Editor::unstage_and_next);
        register_action(editor, window, Editor::expand_all_diff_hunks);
        register_action(editor, window, Editor::collapse_all_diff_hunks);
        register_action(editor, window, Editor::toggle_all_diff_hunks);
        register_action(editor, window, Editor::toggle_review_comments_expanded);
        register_action(editor, window, Editor::submit_diff_review_comment_action);
        register_action(editor, window, Editor::edit_review_comment);
        register_action(editor, window, Editor::delete_review_comment);
        register_action(editor, window, Editor::confirm_edit_review_comment_action);
        register_action(editor, window, Editor::cancel_edit_review_comment_action);
        register_action(editor, window, Editor::go_to_previous_change);
        register_action(editor, window, Editor::go_to_next_change);
        register_action(editor, window, Editor::go_to_prev_reference);
        register_action(editor, window, Editor::go_to_next_reference);
        register_action(editor, window, Editor::go_to_previous_symbol);
        register_action(editor, window, Editor::go_to_next_symbol);
        register_action(editor, window, Editor::restart_language_server);
        register_action(editor, window, Editor::stop_language_server);
        register_action(editor, window, Editor::show_character_palette);
        register_action(editor, window, |editor, action, window, cx| {
            if let Some(task) = editor.compose_completion(action, window, cx) {
                editor.detach_and_notify_err(task, window, cx);
            } else {
                cx.propagate();
            }
        });
        register_action(editor, window, |editor, action, window, cx| {
            if let Some(task) = editor.find_all_references(action, window, cx) {
                task.detach_and_log_err(cx);
            } else {
                cx.propagate();
            }
        });
        register_action(editor, window, Editor::show_signature_help);
        register_action(editor, window, Editor::signature_help_prev);
        register_action(editor, window, Editor::signature_help_next);
        register_action(editor, window, Editor::show_edit_prediction);
        register_action(editor, window, Editor::context_menu_first);
        register_action(editor, window, Editor::context_menu_prev);
        register_action(editor, window, Editor::context_menu_next);
        register_action(editor, window, Editor::context_menu_last);
        register_action(editor, window, Editor::display_cursor_names);
        register_action(editor, window, Editor::open_active_item_in_terminal);
        register_action(editor, window, Editor::spawn_nearest_task);
        register_action(editor, window, Editor::open_selections_in_multibuffer);
        register_action(editor, window, Editor::toggle_bookmark);
        register_action(editor, window, Editor::edit_bookmark);
        register_action(editor, window, Editor::go_to_next_bookmark);
        register_action(editor, window, Editor::go_to_previous_bookmark);
        register_action(editor, window, Editor::toggle_breakpoint);
        register_action(editor, window, Editor::edit_log_breakpoint);
        register_action(editor, window, Editor::enable_breakpoint);
        register_action(editor, window, Editor::disable_breakpoint);
        register_action(editor, window, Editor::toggle_read_only);
        register_action(editor, window, Editor::reload_file);

        if !editor.read(cx).read_only(cx) {
            register_action(editor, window, Editor::newline);
            register_action(editor, window, Editor::newline_above);
            register_action(editor, window, Editor::newline_below);
            register_action(editor, window, Editor::backspace);
            register_action(editor, window, Editor::delete);
            register_action(editor, window, Editor::tab);
            register_action(editor, window, Editor::backtab);
            register_action(editor, window, Editor::indent);
            register_action(editor, window, Editor::outdent);
            register_action(editor, window, Editor::autoindent);
            register_action(editor, window, Editor::delete_line);
            register_action(editor, window, Editor::join_lines);
            register_action(editor, window, Editor::sort_lines_by_length);
            register_action(editor, window, Editor::sort_lines_case_sensitive);
            register_action(editor, window, Editor::sort_lines_case_insensitive);
            register_action(editor, window, Editor::unique_lines_case_insensitive);
            register_action(editor, window, Editor::unique_lines_case_sensitive);
            register_action(editor, window, Editor::reverse_lines);
            register_action(editor, window, Editor::shuffle_lines);
            register_action(editor, window, Editor::rotate_selections_forward);
            register_action(editor, window, Editor::rotate_selections_backward);
            register_action(editor, window, Editor::convert_indentation_to_spaces);
            register_action(editor, window, Editor::convert_indentation_to_tabs);
            register_action(editor, window, Editor::convert_to_upper_case);
            register_action(editor, window, Editor::convert_to_lower_case);
            register_action(editor, window, Editor::convert_to_title_case);
            register_action(editor, window, Editor::convert_to_snake_case);
            register_action(editor, window, Editor::convert_to_kebab_case);
            register_action(editor, window, Editor::convert_to_upper_camel_case);
            register_action(editor, window, Editor::convert_to_lower_camel_case);
            register_action(editor, window, Editor::convert_to_opposite_case);
            register_action(editor, window, Editor::convert_to_sentence_case);
            register_action(editor, window, Editor::toggle_case);
            register_action(editor, window, Editor::convert_to_rot13);
            register_action(editor, window, Editor::convert_to_rot47);
            register_action(editor, window, Editor::convert_to_base64);
            register_action(editor, window, Editor::convert_from_base64);
            register_action(editor, window, Editor::delete_to_previous_word_start);
            register_action(editor, window, Editor::delete_to_previous_subword_start);
            register_action(editor, window, Editor::delete_to_next_word_end);
            register_action(editor, window, Editor::delete_to_next_subword_end);
            register_action(editor, window, Editor::delete_to_beginning_of_line);
            register_action(editor, window, Editor::delete_to_end_of_line);
            register_action(editor, window, Editor::cut_to_end_of_line);
            register_action(editor, window, Editor::duplicate_line_up);
            register_action(editor, window, Editor::duplicate_line_down);
            register_action(editor, window, Editor::duplicate_selection);
            register_action(editor, window, Editor::move_line_up);
            register_action(editor, window, Editor::move_line_down);
            register_action(editor, window, Editor::transpose);
            register_action(editor, window, |editor, _: &crate::Rewrap, _, cx| {
                editor.rewrap(crate::RewrapOptions::default(), cx);
            });
            register_action(editor, window, Editor::cut);
            register_action(editor, window, Editor::kill_ring_cut);
            register_action(editor, window, Editor::kill_ring_yank);
            register_action(editor, window, Editor::paste);
            register_action(editor, window, Editor::undo);
            register_action(editor, window, Editor::redo);
            register_action(editor, window, Editor::toggle_comments);
            register_action(editor, window, Editor::toggle_block_comments);
            register_action(editor, window, Editor::toggle_markdown_block_quote);
            register_action(editor, window, Editor::unwrap_syntax_node);
            register_action(editor, window, Editor::accept_next_word_edit_prediction);
            register_action(editor, window, Editor::accept_next_line_edit_prediction);
            register_action(editor, window, Editor::accept_edit_prediction);
            register_action(editor, window, Editor::restore_file);
            register_action(editor, window, Editor::git_restore);
            register_action(editor, window, Editor::restore_and_next);
            register_action(editor, window, Editor::apply_all_diff_hunks);
            register_action(editor, window, Editor::apply_selected_diff_hunks);
            register_action(editor, window, Editor::insert_uuid_v4);
            register_action(editor, window, Editor::insert_uuid_v7);
            register_action(editor, window, Editor::align_selections);
            if editor.read(cx).enable_wrap_selections_in_tag(cx) {
                register_action(editor, window, Editor::wrap_selections_in_tag);
            }
            register_action(
                editor,
                window,
                |editor, HandleInput(text): &HandleInput, window, cx| {
                    if text.is_empty() {
                        return;
                    }
                    editor.handle_input(text, window, cx);
                },
            );
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.format(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            if editor.read(cx).can_format_selections(cx) {
                register_action(editor, window, |editor, action, window, cx| {
                    if let Some(task) = editor.format_selections(action, window, cx) {
                        editor.detach_and_notify_err(task, window, cx);
                    } else {
                        cx.propagate();
                    }
                });
            }
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.organize_imports(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.confirm_completion(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.confirm_completion_replace(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.confirm_completion_insert(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.confirm_code_action(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.rename(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
            register_action(editor, window, |editor, action, window, cx| {
                if let Some(task) = editor.confirm_rename(action, window, cx) {
                    editor.detach_and_notify_err(task, window, cx);
                } else {
                    cx.propagate();
                }
            });
        }
    }
}
