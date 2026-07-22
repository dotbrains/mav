use super::*;

impl Component for MessageNotification {
    fn scope() -> ComponentScope {
        ComponentScope::Notification
    }

    fn description() -> &'static str {
        "A workspace notification that surfaces a message in a framed container, with an \
        optional title, secondary message, copy button, and primary/secondary action buttons."
    }

    fn preview(_window: &mut Window, cx: &mut App) -> AnyElement {
        let normal =
            cx.new(|cx| MessageNotification::new("A regular informational notification.", cx));

        let with_title = cx.new(|cx| {
            MessageNotification::new("Some informational content for the user.", cx)
                .with_title("Notification Title")
        });

        let with_primary_action = cx.new(|cx| {
            MessageNotification::new("A new version of Mav is available for download.", cx)
                .with_title("Update Available")
                .primary_message("Restart Now")
                .primary_icon(IconName::ArrowCircle)
        });

        let with_end_icon_action = cx.new(|cx| {
            MessageNotification::new("Release notes for this version are available online.", cx)
                .with_title("What’s New")
                .primary_message("Read Release Notes")
                .primary_end_icon(IconName::ArrowUpRight)
        });

        // Mirrors the shape of notifications such as the keymap parse error: a long,
        // multi-line message followed by a primary action button. Useful for catching
        // regressions where the action row overlaps or is clipped by the content above.
        let with_long_content_and_action = cx.new(|cx| {
            let long_message = "Errors in user keymap file. In section with context = \
                \"Workspace\":\n\
                • In binding \"ctrl-r\", expected two-element array of [name, input], \
                found [\"editor::Apply\"].\n\
                • In binding \"ctrl-shift-r\", action \"editor::Reload\" is not registered.";
            MessageNotification::new(long_message, cx)
                .primary_message("Open Keymap File")
                .primary_icon(IconName::Settings)
        });

        struct PreviewError;
        impl WorkspaceError for PreviewError {
            fn primary_message(&self) -> SharedString {
                "Something went wrong while loading your project.".into()
            }

            fn primary_action(&self) -> ErrorAction {
                ErrorAction::dismiss()
            }

            fn secondary_message(&self) -> Option<SharedString> {
                Some("Check your network connection and try again.".into())
            }
            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Error
            }
        }
        let error_state = cx.new(|cx| MessageNotification::from_workspace_error(PreviewError, cx));

        let close_only =
            cx.new(|cx| MessageNotification::new("Default header with just a close button.", cx));

        let copy_and_close = cx.new(|cx| {
            let msg: SharedString = "This message can be copied to the clipboard.".into();
            MessageNotification::new(msg.clone(), cx).copy_text(msg)
        });

        let no_close = cx.new(|cx| {
            MessageNotification::new("This notification can't be closed manually.", cx)
                .show_close_button(false)
        });

        // --- Workspace errors ---
        // These showcase common shapes of [`WorkspaceError`]. They are intentionally
        // [`ErrorSeverity::Critical`] so they never auto-dismiss in the preview, which
        // would otherwise make them disappear mid-inspection.

        struct BasicError;
        impl WorkspaceError for BasicError {
            fn primary_message(&self) -> SharedString {
                "Failed to save the file.".into()
            }
            fn primary_action(&self) -> ErrorAction {
                ErrorAction::dismiss()
            }
            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Critical
            }
        }

        struct LanguageServerError;
        impl WorkspaceError for LanguageServerError {
            fn primary_message(&self) -> SharedString {
                "Error: Prepare rename via rust-analyzer failed: No references found at position"
                    .into()
            }
            fn secondary_message(&self) -> Option<SharedString> {
                None
            }
            fn primary_action(&self) -> ErrorAction {
                ErrorAction::dismiss()
            }
            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Critical
            }
        }

        // Mirrors the shape of [`super::super::PortalError`]: a critical error with a
        // documentation link as its primary action.
        struct PortalSetupError;
        impl WorkspaceError for PortalSetupError {
            fn primary_message(&self) -> SharedString {
                "Linux desktop portal initialization failed.".into()
            }
            fn secondary_message(&self) -> Option<SharedString> {
                Some("Mav needs an xdg-desktop-portal implementation to open files.".into())
            }
            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Critical
            }
            fn primary_action(&self) -> ErrorAction {
                ErrorAction::link(
                    "See Docs",
                    "https://mav.dev/docs/linux#i-cant-open-any-files",
                )
            }
        }

        // Has both a primary action (link) and a secondary action (dismiss), so the
        // preview exercises the full button row.
        struct UpdateRequiredError;
        impl WorkspaceError for UpdateRequiredError {
            fn primary_message(&self) -> SharedString {
                "An update is required to continue using Mav AI.".into()
            }
            fn severity(&self) -> ErrorSeverity {
                ErrorSeverity::Critical
            }
            fn primary_action(&self) -> ErrorAction {
                ErrorAction::link("Update Mav", "https://mav.dev/releases")
            }
            fn secondary_action(&self) -> Option<ErrorAction> {
                Some(ErrorAction::dismiss())
            }
        }

        let basic_error = cx.new(|cx| MessageNotification::from_workspace_error(BasicError, cx));
        let detailed_error =
            cx.new(|cx| MessageNotification::from_workspace_error(LanguageServerError, cx));
        let docs_error =
            cx.new(|cx| MessageNotification::from_workspace_error(PortalSetupError, cx));
        let update_error =
            cx.new(|cx| MessageNotification::from_workspace_error(UpdateRequiredError, cx));

        let container = || div().w(px(440.));

        v_flex()
            .gap_6()
            .p_4()
            .children(vec![
                example_group_with_title(
                    "States",
                    vec![
                        single_example("Normal", container().child(normal).into_any_element()),
                        single_example(
                            "With Title",
                            container().child(with_title).into_any_element(),
                        ),
                        single_example(
                            "With Primary Action (start icon)",
                            container().child(with_primary_action).into_any_element(),
                        ),
                        single_example(
                            "With Primary Action (end icon)",
                            container().child(with_end_icon_action).into_any_element(),
                        ),
                        single_example(
                            "Long Content + Primary Action",
                            container()
                                .child(with_long_content_and_action)
                                .into_any_element(),
                        ),
                        single_example("Error", container().child(error_state).into_any_element()),
                    ],
                ),
                example_group_with_title(
                    "Header Actions (top right)",
                    vec![
                        single_example(
                            "Close Only",
                            container().child(close_only).into_any_element(),
                        ),
                        single_example(
                            "Copy + Close",
                            container().child(copy_and_close).into_any_element(),
                        ),
                        single_example("No Close", container().child(no_close).into_any_element()),
                    ],
                ),
                example_group_with_title(
                    "Workspace Errors",
                    vec![
                        single_example("Basic", container().child(basic_error).into_any_element()),
                        single_example(
                            "With Secondary Message",
                            container().child(detailed_error).into_any_element(),
                        ),
                        single_example(
                            "With Documentation Link",
                            container().child(docs_error).into_any_element(),
                        ),
                        single_example(
                            "With Primary + Secondary Action",
                            container().child(update_error).into_any_element(),
                        ),
                    ],
                ),
            ])
            .into_any_element()
    }
}
