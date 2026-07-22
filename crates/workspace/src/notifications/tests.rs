#[cfg(test)]
mod tests {
    use std::time::Duration;

    use fs::FakeFs;
    use gpui::TestAppContext;
    use project::{LanguageServerPromptRequest, Project};

    use crate::tests::init_test;

    use super::*;

    #[gpui::test]
    async fn test_notification_auto_dismiss_with_notifications_from_multiple_language_servers(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

        let count_notifications = |workspace: &Entity<Workspace>, cx: &mut TestAppContext| {
            workspace.read_with(cx, |workspace, _| workspace.notification_ids().len())
        };

        let show_notification = |workspace: &Entity<Workspace>,
                                 cx: &mut TestAppContext,
                                 lsp_name: &str| {
            workspace.update(cx, |workspace, cx| {
                let request = LanguageServerPromptRequest::test(
                    gpui::PromptLevel::Warning,
                    "Test notification".to_string(),
                    vec![], // Empty actions triggers auto-dismiss
                    lsp_name.to_string(),
                );
                let notification_id = NotificationId::composite::<LanguageServerPrompt>(request.id);
                workspace.show_notification(notification_id, cx, |cx| {
                    cx.new(|cx| LanguageServerPrompt::new(request, cx))
                });
            })
        };

        show_notification(&workspace, cx, "Lsp1");
        assert_eq!(count_notifications(&workspace, cx), 1);

        cx.executor().advance_clock(Duration::from_millis(1000));

        show_notification(&workspace, cx, "Lsp2");
        assert_eq!(count_notifications(&workspace, cx), 2);

        cx.executor().advance_clock(Duration::from_millis(1000));

        show_notification(&workspace, cx, "Lsp3");
        assert_eq!(count_notifications(&workspace, cx), 3);

        cx.executor().advance_clock(Duration::from_millis(3000));
        assert_eq!(count_notifications(&workspace, cx), 2);

        cx.executor().advance_clock(Duration::from_millis(1000));
        assert_eq!(count_notifications(&workspace, cx), 1);

        cx.executor().advance_clock(Duration::from_millis(1000));
        assert_eq!(count_notifications(&workspace, cx), 0);
    }

    #[gpui::test]
    async fn test_notification_auto_dismiss_with_multiple_notifications_from_single_language_server(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let lsp_name = "server1";

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

        let count_notifications = |workspace: &Entity<Workspace>, cx: &mut TestAppContext| {
            workspace.read_with(cx, |workspace, _| workspace.notification_ids().len())
        };

        let show_notification = |lsp_name: &str,
                                 workspace: &Entity<Workspace>,
                                 cx: &mut TestAppContext| {
            workspace.update(cx, |workspace, cx| {
                let lsp_name = lsp_name.to_string();
                let request = LanguageServerPromptRequest::test(
                    gpui::PromptLevel::Warning,
                    "Test notification".to_string(),
                    vec![], // Empty actions triggers auto-dismiss
                    lsp_name,
                );
                let notification_id = NotificationId::composite::<LanguageServerPrompt>(request.id);

                workspace.show_notification(notification_id, cx, |cx| {
                    cx.new(|cx| LanguageServerPrompt::new(request, cx))
                });
            })
        };

        show_notification(lsp_name, &workspace, cx);
        assert_eq!(count_notifications(&workspace, cx), 1);

        cx.executor().advance_clock(Duration::from_millis(1000));

        show_notification(lsp_name, &workspace, cx);
        assert_eq!(count_notifications(&workspace, cx), 2);

        cx.executor().advance_clock(Duration::from_millis(4000));
        assert_eq!(count_notifications(&workspace, cx), 1);

        cx.executor().advance_clock(Duration::from_millis(1000));
        assert_eq!(count_notifications(&workspace, cx), 0);
    }

    #[gpui::test]
    async fn test_notification_auto_dismiss_turned_off(cx: &mut TestAppContext) {
        init_test(cx);

        cx.update(|cx| {
            let mut settings = ProjectSettings::get_global(cx).clone();
            settings
                .global_lsp_settings
                .notifications
                .dismiss_timeout_ms = Some(0);
            ProjectSettings::override_global(settings, cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

        let count_notifications = |workspace: &Entity<Workspace>, cx: &mut TestAppContext| {
            workspace.read_with(cx, |workspace, _| workspace.notification_ids().len())
        };

        workspace.update(cx, |workspace, cx| {
            let request = LanguageServerPromptRequest::test(
                gpui::PromptLevel::Warning,
                "Test notification".to_string(),
                vec![], // Empty actions would trigger auto-dismiss if enabled
                "test_server".to_string(),
            );
            let notification_id = NotificationId::composite::<LanguageServerPrompt>(request.id);
            workspace.show_notification(notification_id, cx, |cx| {
                cx.new(|cx| LanguageServerPrompt::new(request, cx))
            });
        });

        assert_eq!(count_notifications(&workspace, cx), 1);

        // Advance time beyond the default auto-dismiss duration
        cx.executor().advance_clock(Duration::from_millis(10000));
        assert_eq!(count_notifications(&workspace, cx), 1);
    }

    #[gpui::test]
    async fn test_notification_auto_dismiss_with_custom_duration(cx: &mut TestAppContext) {
        init_test(cx);

        let custom_duration_ms: u64 = 2000;
        cx.update(|cx| {
            let mut settings = ProjectSettings::get_global(cx).clone();
            settings
                .global_lsp_settings
                .notifications
                .dismiss_timeout_ms = Some(custom_duration_ms);
            ProjectSettings::override_global(settings, cx);
        });

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let (workspace, cx) =
            cx.add_window_view(|window, cx| Workspace::test_new(project.clone(), window, cx));

        let count_notifications = |workspace: &Entity<Workspace>, cx: &mut TestAppContext| {
            workspace.read_with(cx, |workspace, _| workspace.notification_ids().len())
        };

        workspace.update(cx, |workspace, cx| {
            let request = LanguageServerPromptRequest::test(
                gpui::PromptLevel::Warning,
                "Test notification".to_string(),
                vec![], // Empty actions triggers auto-dismiss
                "test_server".to_string(),
            );
            let notification_id = NotificationId::composite::<LanguageServerPrompt>(request.id);
            workspace.show_notification(notification_id, cx, |cx| {
                cx.new(|cx| LanguageServerPrompt::new(request, cx))
            });
        });

        assert_eq!(count_notifications(&workspace, cx), 1);

        // Advance time less than custom duration
        cx.executor()
            .advance_clock(Duration::from_millis(custom_duration_ms - 500));
        assert_eq!(count_notifications(&workspace, cx), 1);

        // Advance time past the custom duration
        cx.executor().advance_clock(Duration::from_millis(1000));
        assert_eq!(count_notifications(&workspace, cx), 0);
    }
}
