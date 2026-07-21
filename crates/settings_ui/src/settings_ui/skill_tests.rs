use super::*;

#[gpui::test]
async fn test_skills_page_scope_switch_updates_displayed_skills(cx: &mut gpui::TestAppContext) {
    use agent_skills::{
        ProjectSkillGroup, Skill, SkillScopeId, SkillSource, load_skills_from_directory,
    };
    use project::Project;
    use serde_json::json;
    use std::path::Path;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    let fake_fs = app_state.fs.as_fake();

    fake_fs
            .insert_tree(
                "/global-skills",
                json!({
                    "global-skill": {
                        "SKILL.md": "---\nname: global-skill\ndescription: A user level skill\n---\n\nGlobal instructions."
                    }
                }),
            )
            .await;

    fake_fs
            .insert_tree(
                "/project",
                json!({
                    ".agents": {
                        "skills": {
                            "project-skill": {
                                "SKILL.md": "---\nname: project-skill\ndescription: A project level skill\n---\n\nProject instructions."
                            }
                        }
                    },
                    "main.rs": "fn main() {}"
                }),
            )
            .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    // Load both skills from the fake filesystem the same way the agent
    // does, then publish them as the global skill index.
    let fs = app_state.fs.clone();
    let global_skills: Vec<Skill> =
        load_skills_from_directory(&fs, Path::new("/global-skills"), SkillSource::Global)
            .await
            .into_iter()
            .map(|result| result.expect("global skill should load"))
            .collect();
    let project_skills: Vec<Skill> = load_skills_from_directory(
        &fs,
        Path::new("/project/.agents/skills"),
        SkillSource::ProjectLocal {
            worktree_id: SkillScopeId(worktree_id.to_usize()),
            worktree_root_name: "project".into(),
        },
    )
    .await
    .into_iter()
    .map(|result| result.expect("project skill should load"))
    .collect();
    assert_eq!(global_skills.len(), 1);
    assert_eq!(project_skills.len(), 1);

    cx.update(|cx| {
        cx.set_global(SkillIndex {
            global_skills,
            project_skills: vec![ProjectSkillGroup {
                worktree_id: SkillScopeId(worktree_id.to_usize()),
                worktree_root_name: "project".into(),
                skills: project_skills,
            }],
        });
    });

    let (_multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });
    let workspace_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace_handle), window, cx));

    cx.run_until_parked();

    settings_window.update_in(cx, |settings_window, window, cx| {
        fn displayed_skill_names(settings_window: &SettingsWindow, cx: &App) -> Vec<String> {
            crate::pages::displayed_skills(settings_window, cx)
                .iter()
                .map(|skill| skill.name.to_string())
                .collect()
        }

        assert_eq!(settings_window.current_file, SettingsUiFile::User);
        assert!(
            settings_window.navigate_to_sub_page(AGENT_SKILLS_SETTINGS_PATH, window, cx),
            "Skills sub-page should exist"
        );
        assert_eq!(displayed_skill_names(settings_window, cx), ["global-skill"]);

        let project_file_index = settings_window
            .files
            .iter()
            .position(|(file, _)| file.worktree_id() == Some(worktree_id))
            .expect("project settings file should be listed");
        settings_window.change_file_in_sub_page(project_file_index, window, cx);

        assert_eq!(
            settings_window.current_file.worktree_id(),
            Some(worktree_id)
        );
        assert_eq!(
            settings_window.sub_page_stack.len(),
            1,
            "Skills sub-page should stay open when switching scope"
        );
        assert_eq!(settings_window.sub_page_stack[0].link.title, "Skills");
        assert_eq!(
            displayed_skill_names(settings_window, cx),
            ["project-skill"]
        );

        let user_file_index = settings_window
            .files
            .iter()
            .position(|(file, _)| file == &SettingsUiFile::User)
            .expect("user settings file should be listed");
        settings_window.change_file_in_sub_page(user_file_index, window, cx);

        assert_eq!(settings_window.current_file, SettingsUiFile::User);
        assert_eq!(settings_window.sub_page_stack.len(), 1);
        assert_eq!(displayed_skill_names(settings_window, cx), ["global-skill"]);
    });
}

#[gpui::test]
async fn test_open_skill_creator_navigates_to_sub_page(cx: &mut gpui::TestAppContext) {
    use project::Project;

    cx.update(|cx| {
        register_settings(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    app_state
        .fs
        .as_fake()
        .insert_tree("/project", serde_json::json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");

    let (_multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });
    let workspace_handle = cx.window_handle().downcast::<MultiWorkspace>().unwrap();

    cx.run_until_parked();

    let (settings_window, cx) =
        cx.add_window_view(|window, cx| SettingsWindow::new(Some(workspace_handle), window, cx));

    cx.run_until_parked();

    settings_window.update_in(cx, |settings_window, window, cx| {
        settings_window.navigate_to_skill_creator(pages::SkillCreatorOpenMode::Form, window, cx);
    });

    cx.run_until_parked();

    settings_window.read_with(cx, |settings_window, _| {
        let titles: Vec<_> = settings_window
            .sub_page_stack
            .iter()
            .map(|sub_page| sub_page.link.title.to_string())
            .collect();
        assert_eq!(
            titles,
            ["Skills", "Create Skill"],
            "skill creator should be pushed on top of the skills page"
        );
        assert!(
            settings_window.skill_creator_page().is_some(),
            "skill creator page state should exist"
        );
    });
}

#[gpui::test]
async fn test_open_skill_creator_action_opens_settings_window_at_sub_page(
    cx: &mut gpui::TestAppContext,
) {
    use project::Project;

    cx.update(|cx| {
        register_settings(cx);
        release_channel::init("0.0.0".parse().unwrap(), cx);
        crate::init(cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = AppState::test(cx);
        AppState::set_global(app_state.clone(), cx);
        app_state
    });

    app_state
        .fs
        .as_fake()
        .insert_tree("/project", serde_json::json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = cx.update(|cx| {
        Project::local(
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            None,
            project::LocalProjectFlags::default(),
            cx,
        )
    });
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project", true, cx)
        })
        .await
        .expect("Failed to create worktree");

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        let workspace = cx.new(|cx| {
            Workspace::new(
                Default::default(),
                project.clone(),
                app_state.clone(),
                window,
                cx,
            )
        });
        MultiWorkspace::new(workspace, window, cx)
    });

    cx.run_until_parked();

    // Dispatch the action the way the command palette does: on the
    // workspace window.
    multi_workspace.update_in(cx, |_multi_workspace, window, cx| {
        window.dispatch_action(Box::new(mav_actions::assistant::OpenSkillCreator), cx);
    });

    cx.run_until_parked();

    let settings_window = cx
        .update(|_, cx| {
            cx.windows()
                .into_iter()
                .find_map(|window| window.downcast::<SettingsWindow>())
        })
        .expect("dispatching agent::OpenSkillCreator should open the settings window");

    settings_window
        .read_with(cx, |settings_window, _| {
            let titles: Vec<_> = settings_window
                .sub_page_stack
                .iter()
                .map(|sub_page| sub_page.link.title.to_string())
                .collect();
            assert_eq!(
                titles,
                ["Skills", "Create Skill"],
                "skill creator should be pushed on top of the skills page"
            );
        })
        .unwrap();
}
