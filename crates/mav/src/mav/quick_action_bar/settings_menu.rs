use super::*;

impl QuickActionBar {
    pub(super) fn render_editor_settings_dropdown(
        &self,
        editor: WeakEntity<Editor>,
        editor_focus_handle: FocusHandle,
        supports_inlay_hints: bool,
        inlay_hints_enabled: bool,
        inline_values_enabled: bool,
        supports_semantic_tokens: bool,
        semantic_highlights_enabled: bool,
        supports_code_lens: bool,
        code_lens_enabled: bool,
        supports_minimap: bool,
        minimap_enabled: bool,
        has_edit_prediction_provider: bool,
        edit_predictions_enabled_at_cursor: bool,
        show_edit_predictions: bool,
        is_full: bool,
        diagnostics_enabled: bool,
        supports_inline_diagnostics: bool,
        inline_diagnostics_enabled: bool,
        show_line_numbers: bool,
        selection_menu_enabled: bool,
        auto_signature_help_enabled: bool,
        git_blame_inline_enabled: bool,
        show_git_blame_gutter: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let vim_mode_enabled = VimModeSetting::get_global(cx).0;
        let helix_mode_enabled = HelixModeSetting::get_global(cx).0;

        PopoverMenu::new("editor-settings")
                .trigger_with_tooltip(
                    IconButton::new("toggle_editor_settings_icon", IconName::Sliders)
                        .icon_size(IconSize::Small)
                        .style(ButtonStyle::Subtle)
                        .toggle_state(self.toggle_settings_handle.is_deployed()),
                    Tooltip::text("Editor Controls"),
                )
                .anchor(Anchor::TopRight)
                .with_handle(self.toggle_settings_handle.clone())
                .menu(move |window, cx| {
                    let menu = ContextMenu::build(window, cx, {
                        let focus_handle = editor_focus_handle.clone();
                        |mut menu, _, _| {
                            menu = menu.context(focus_handle);

                            if supports_inlay_hints {
                                menu = menu.toggleable_entry(
                                    "Inlay Hints",
                                    inlay_hints_enabled,
                                    IconPosition::Start,
                                    Some(editor::actions::ToggleInlayHints.boxed_clone()),
                                    {
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_inlay_hints(
                                                        &editor::actions::ToggleInlayHints,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    },
                                );

                                menu = menu.toggleable_entry(
                                    "Inline Values",
                                    inline_values_enabled,
                                    IconPosition::Start,
                                    Some(editor::actions::ToggleInlineValues.boxed_clone()),
                                    {
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_inline_values(
                                                        &editor::actions::ToggleInlineValues,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    }
                                );
                            }

                            if supports_semantic_tokens {
                                menu = menu.toggleable_entry(
                                    "Semantic Highlights",
                                    semantic_highlights_enabled,
                                    IconPosition::Start,
                                    Some(editor::actions::ToggleSemanticHighlights.boxed_clone()),
                                    {
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_semantic_highlights(
                                                        &editor::actions::ToggleSemanticHighlights,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    },
                                );
                            }

                            if supports_code_lens {
                                menu = menu.toggleable_entry(
                                    "Code Lens",
                                    code_lens_enabled,
                                    IconPosition::Start,
                                    Some(editor::actions::ToggleCodeLens.boxed_clone()),
                                    {
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_code_lens_action(
                                                        &editor::actions::ToggleCodeLens,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    },
                                );
                            }

                            if supports_minimap {
                                menu = menu.toggleable_entry("Minimap", minimap_enabled, IconPosition::Start, Some(editor::actions::ToggleMinimap.boxed_clone()), {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_minimap(
                                                    &editor::actions::ToggleMinimap,
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                    }
                                },)
                            }

                            if has_edit_prediction_provider {
                                let mut edit_prediction_entry = ContextMenuEntry::new("Edit Predictions")
                                    .toggleable(IconPosition::Start, edit_predictions_enabled_at_cursor && show_edit_predictions)
                                    .disabled(!edit_predictions_enabled_at_cursor)
                                    .action(
                                        editor::actions::ToggleEditPrediction.boxed_clone(),
                                    ).handler({
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_edit_predictions(
                                                        &editor::actions::ToggleEditPrediction,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    });
                                if !edit_predictions_enabled_at_cursor {
                                    edit_prediction_entry = edit_prediction_entry.documentation_aside(DocumentationSide::Left, |_| {
                                        Label::new("You can't toggle edit predictions for this file as it is within the excluded files list.").into_any_element()
                                    });
                                }

                                menu = menu.item(edit_prediction_entry);
                            }

                            menu = menu.separator();

                            if is_full {
                                menu = menu.toggleable_entry(
                                    "Diagnostics",
                                    diagnostics_enabled,
                                    IconPosition::Start,
                                    Some(ToggleDiagnostics.boxed_clone()),
                                    {
                                        let editor = editor.clone();
                                        move |window, cx| {
                                            editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_diagnostics(
                                                        &ToggleDiagnostics,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    },
                                );

                                if supports_inline_diagnostics {
                                    let mut inline_diagnostics_item = ContextMenuEntry::new("Inline Diagnostics")
                                        .toggleable(IconPosition::Start, diagnostics_enabled && inline_diagnostics_enabled)
                                        .action(ToggleInlineDiagnostics.boxed_clone())
                                        .handler({
                                            let editor = editor.clone();
                                            move |window, cx| {
                                                editor
                                                    .update(cx, |editor, cx| {
                                                        editor.toggle_inline_diagnostics(
                                                            &ToggleInlineDiagnostics,
                                                            window,
                                                            cx,
                                                        );
                                                    })
                                                    .ok();
                                            }
                                        });
                                    if !diagnostics_enabled {
                                        inline_diagnostics_item = inline_diagnostics_item.disabled(true).documentation_aside(DocumentationSide::Left, |_|  Label::new("Inline diagnostics are not available until regular diagnostics are enabled.").into_any_element());
                                    }
                                    menu = menu.item(inline_diagnostics_item)
                                }

                                menu = menu.separator();
                            }

                            menu = menu.toggleable_entry(
                                "Line Numbers",
                                show_line_numbers,
                                IconPosition::Start,
                                Some(editor::actions::ToggleLineNumbers.boxed_clone()),
                                {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_line_numbers(
                                                    &editor::actions::ToggleLineNumbers,
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                    }
                                },
                            );

                            menu = menu.toggleable_entry(
                                "Selection Menu",
                                selection_menu_enabled,
                                IconPosition::Start,
                                Some(editor::actions::ToggleSelectionMenu.boxed_clone()),
                                {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_selection_menu(
                                                    &editor::actions::ToggleSelectionMenu,
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .ok();
                                    }
                                },
                            );

                            menu = menu.toggleable_entry(
                                "Auto Signature Help",
                                auto_signature_help_enabled,
                                IconPosition::Start,
                                Some(editor::actions::ToggleAutoSignatureHelp.boxed_clone()),
                                {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_auto_signature_help_menu(
                                                    &editor::actions::ToggleAutoSignatureHelp,
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                    }
                                },
                            );

                            menu = menu.separator();

                            menu = menu.toggleable_entry(
                                "Inline Git Blame",
                                git_blame_inline_enabled,
                                IconPosition::Start,
                                Some(editor::actions::ToggleGitBlameInline.boxed_clone()),
                                {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_git_blame_inline(
                                                    &editor::actions::ToggleGitBlameInline,
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .ok();
                                    }
                                },
                            );

                            menu = menu.toggleable_entry(
                                "Column Git Blame",
                                show_git_blame_gutter,
                                IconPosition::Start,
                                Some(git::Blame.boxed_clone()),
                                {
                                    let editor = editor.clone();
                                    move |window, cx| {
                                        editor
                                            .update(cx, |editor, cx| {
                                                editor.toggle_git_blame(
                                                    &git::Blame,
                                                    window,
                                                    cx,
                                                )
                                            })
                                            .ok();
                                    }
                                },
                            );

                            menu = menu.separator();

                            menu = menu.toggleable_entry(
                                "Vim Mode",
                                vim_mode_enabled,
                                IconPosition::Start,
                                None,
                                {
                                    move |window, cx| {
                                        let new_value = !vim_mode_enabled;
                                        VimModeSetting::override_global(VimModeSetting(new_value), cx);
                                        HelixModeSetting::override_global(HelixModeSetting(false), cx);
                                        window.refresh();
                                    }
                                },
                            );
                            menu = menu.toggleable_entry(
                                "Helix Mode",
                                helix_mode_enabled,
                                IconPosition::Start,
                                None,
                                {
                                    move |window, cx| {
                                        let new_value = !helix_mode_enabled;
                                        HelixModeSetting::override_global(HelixModeSetting(new_value), cx);
                                        VimModeSetting::override_global(VimModeSetting(false), cx);
                                        window.refresh();
                                    }
                                }
                            );

                            menu
                        }
                    });
                    Some(menu)
                })
    }
}
