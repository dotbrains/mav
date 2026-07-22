use super::*;

pub(super) struct ActionArgumentsEditor {
    pub(super) editor: Entity<Editor>,
    focus_handle: FocusHandle,
    is_loading: bool,
    /// See documentation in `KeymapEditor` for why a temp dir is needed.
    /// This field exists because the keymap editor temp dir creation may fail,
    /// and rather than implement a complicated retry mechanism, we simply
    /// fallback to trying to create a temporary directory in this editor on
    /// demand. Of note is that the TempDir struct will remove the directory
    /// when dropped.
    backup_temp_dir: Option<tempfile::TempDir>,
}

impl Focusable for ActionArgumentsEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ActionArgumentsEditor {
    pub(super) fn new(
        action_name: &'static str,
        arguments: Option<SharedString>,
        temp_dir: Option<&std::path::Path>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus_in(&focus_handle, window, |this, window, cx| {
            this.editor.focus_handle(cx).focus(window, cx);
        })
        .detach();
        let editor = cx.new(|cx| {
            let mut editor = Editor::auto_height_unbounded(1, window, cx);
            Self::set_editor_text(&mut editor, arguments.clone(), window, cx);
            editor.set_read_only(true);
            editor
        });

        let temp_dir = temp_dir.map(|path| path.to_owned());
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let (project, fs) = workspace.read_with(cx, |workspace, _cx| {
                    (
                        workspace.project().downgrade(),
                        workspace.app_state().fs.clone(),
                    )
                })?;

                let file_name = json_schema_store::normalized_action_file_name(action_name);

                let (buffer, backup_temp_dir) =
                    Self::create_temp_buffer(temp_dir, file_name.clone(), project.clone(), fs, cx)
                        .await
                        .context(concat!(
                            "Failed to create temporary buffer for action arguments. ",
                            "Auto-complete will not work"
                        ))?;

                let editor = cx.new_window_entity(|window, cx| {
                    let multi_buffer = cx.new(|cx| editor::MultiBuffer::singleton(buffer, cx));
                    let mut editor = Editor::new(
                        EditorMode::Full {
                            scale_ui_elements_with_buffer_font_size: true,
                            show_active_line_background: false,
                            sizing_behavior: SizingBehavior::SizeByContent,
                        },
                        multi_buffer,
                        project.upgrade(),
                        window,
                        cx,
                    );
                    editor.disable_mouse_wheel_zoom();
                    editor.set_searchable(false);
                    editor.disable_scrollbars_and_minimap(window, cx);
                    editor.set_show_edit_predictions(Some(false), window, cx);
                    editor.set_show_gutter(false, cx);
                    Self::set_editor_text(&mut editor, arguments, window, cx);
                    editor
                })?;

                this.update_in(cx, |this, window, cx| {
                    if this.editor.focus_handle(cx).is_focused(window) {
                        editor.focus_handle(cx).focus(window, cx);
                    }
                    this.editor = editor;
                    this.backup_temp_dir = backup_temp_dir;
                    this.is_loading = false;
                })?;

                anyhow::Ok(())
            }
            .await;
            if result.is_err() {
                let json_language = load_json_language(workspace.clone(), cx).await;
                this.update(cx, |this, cx| {
                    this.editor.update(cx, |editor, cx| {
                        if let Some(buffer) = editor.buffer().read(cx).as_singleton() {
                            buffer.update(cx, |buffer, cx| {
                                buffer.set_language(Some(json_language.clone()), cx)
                            });
                        }
                    })
                    // .context("Failed to load JSON language for editing keybinding action arguments input")
                })
                .ok();
                this.update(cx, |this, _cx| {
                    this.is_loading = false;
                })
                .ok();
            }
            result
        })
        .detach_and_log_err(cx);
        Self {
            editor,
            focus_handle,
            is_loading: true,
            backup_temp_dir: None,
        }
    }

    fn set_editor_text(
        editor: &mut Editor,
        arguments: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if let Some(arguments) = arguments {
            editor.set_text(arguments, window, cx);
        } else {
            // TODO: default value from schema?
            editor.set_placeholder_text("Action Arguments", window, cx);
        }
    }

    async fn create_temp_buffer(
        temp_dir: Option<std::path::PathBuf>,
        file_name: String,
        project: WeakEntity<Project>,
        fs: Arc<dyn Fs>,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<(Entity<language::Buffer>, Option<tempfile::TempDir>)> {
        let (temp_file_path, temp_dir) = {
            let file_name = file_name.clone();
            async move {
                let temp_dir_backup = match temp_dir.as_ref() {
                    Some(_) => None,
                    None => {
                        let temp_dir = paths::temp_dir();
                        let sub_temp_dir = tempfile::Builder::new()
                            .tempdir_in(temp_dir)
                            .context("Failed to create temporary directory")?;
                        Some(sub_temp_dir)
                    }
                };
                let dir_path = temp_dir.as_deref().unwrap_or_else(|| {
                    temp_dir_backup
                        .as_ref()
                        .expect("created backup tempdir")
                        .path()
                });
                let path = dir_path.join(file_name);
                fs.create_file(
                    &path,
                    fs::CreateOptions {
                        ignore_if_exists: true,
                        overwrite: true,
                    },
                )
                .await
                .context("Failed to create temporary file")?;
                anyhow::Ok((path, temp_dir_backup))
            }
        }
        .await
        .context("Failed to create backing file")?;

        project
            .update(cx, |project, cx| {
                project.open_local_buffer(temp_file_path, cx)
            })?
            .await
            .context("Failed to create buffer")
            .map(|buffer| (buffer, temp_dir))
    }
}

impl Render for ActionArgumentsEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = theme_settings::ThemeSettings::get_global(cx);
        let colors = cx.theme().colors();

        let border_color = if self.is_loading {
            colors.border_disabled
        } else if self.focus_handle.contains_focused(window, cx) {
            colors.border_focused
        } else {
            colors.border_variant
        };

        let text_style = {
            TextStyleRefinement {
                font_size: Some(rems(0.875).into()),
                font_weight: Some(settings.buffer_font.weight),
                line_height: Some(relative(1.2)),
                color: self.is_loading.then_some(colors.text_disabled),
                ..Default::default()
            }
        };

        self.editor
            .update(cx, |editor, _| editor.set_text_style_refinement(text_style));

        h_flex()
            .min_h_8()
            .min_w_48()
            .px_2()
            .flex_grow_1()
            .rounded_md()
            .bg(cx.theme().colors().editor_background)
            .border_1()
            .border_color(border_color)
            .track_focus(&self.focus_handle)
            .child(self.editor.clone())
    }
}
