use super::*;

#[derive(IntoElement)]
struct SectionHeader {
    title: SharedString,
}

impl SectionHeader {
    fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .px_1()
            .mb_2()
            .gap_2()
            .child(
                Label::new(self.title.to_ascii_uppercase())
                    .buffer_font(cx)
                    .color(Color::Muted)
                    .size(LabelSize::XSmall),
            )
            .child(Divider::horizontal().color(DividerColor::BorderVariant))
    }
}

#[derive(IntoElement)]
struct SectionButton {
    label: SharedString,
    icon: IconName,
    action: Box<dyn Action>,
    tab_index: usize,
    focus_handle: FocusHandle,
}

impl SectionButton {
    fn new(
        label: impl Into<SharedString>,
        icon: IconName,
        action: &dyn Action,
        tab_index: usize,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            label: label.into(),
            icon,
            action: action.boxed_clone(),
            tab_index,
            focus_handle,
        }
    }
}

impl RenderOnce for SectionButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = format!("onb-button-{}-{}", self.label, self.tab_index);
        let action_ref: &dyn Action = &*self.action;

        ButtonLike::new(id)
            .tab_index(self.tab_index as isize)
            .full_width()
            .size(ButtonSize::Medium)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Icon::new(self.icon)
                                    .color(Color::Muted)
                                    .size(IconSize::Small),
                            )
                            .child(Label::new(self.label)),
                    )
                    .child(
                        KeyBinding::for_action_in(action_ref, &self.focus_handle, cx)
                            .size(rems_from_px(12.)),
                    ),
            )
            .on_click(move |_, window, cx| {
                self.focus_handle.dispatch_action(&*self.action, window, cx)
            })
    }
}

enum SectionVisibility {
    Always,
}

impl SectionVisibility {
    fn is_visible(&self) -> bool {
        match self {
            SectionVisibility::Always => true,
        }
    }
}

struct SectionEntry {
    icon: IconName,
    title: &'static str,
    action: &'static dyn Action,
    visibility_guard: SectionVisibility,
}

impl SectionEntry {
    fn render(&self, button_index: usize, focus: &FocusHandle) -> Option<impl IntoElement> {
        self.visibility_guard.is_visible().then(|| {
            SectionButton::new(
                self.title,
                self.icon,
                self.action,
                button_index,
                focus.clone(),
            )
        })
    }
}

const CONTENT: (Section<4>, Section<3>) = (
    Section {
        title: "Get Started",
        entries: [
            SectionEntry {
                icon: IconName::Plus,
                title: "New File",
                action: &NewFile,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::FolderOpen,
                title: "Open Project",
                action: &Open::DEFAULT,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::CloudDownload,
                title: "Clone Repository",
                action: &GitClone,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::ListCollapse,
                title: "Open Command Palette",
                action: &command_palette::Toggle,
                visibility_guard: SectionVisibility::Always,
            },
        ],
    },
    Section {
        title: "Configure",
        entries: [
            SectionEntry {
                icon: IconName::Settings,
                title: "Open Settings",
                action: &OpenSettings,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::Keyboard,
                title: "Customize Keymaps",
                action: &OpenKeymap,
                visibility_guard: SectionVisibility::Always,
            },
            SectionEntry {
                icon: IconName::Blocks,
                title: "Explore Extensions",
                action: &Extensions {
                    category_filter: None,
                    id: None,
                },
                visibility_guard: SectionVisibility::Always,
            },
        ],
    },
);

struct Section<const COLS: usize> {
    title: &'static str,
    entries: [SectionEntry; COLS],
}

impl<const COLS: usize> Section<COLS> {
    fn render(self, index_offset: usize, focus: &FocusHandle) -> impl IntoElement {
        v_flex()
            .min_w_full()
            .child(SectionHeader::new(self.title))
            .children(
                self.entries
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entry)| entry.render(index_offset + index, focus)),
            )
    }
}

impl WelcomePage {
    fn render_agent_card(&self, tab_index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus_handle.clone();
        let color = cx.theme().colors();

        let description = "Run multiple threads at once, mix and match any ACP-compatible agent, and keep work conflict-free with worktrees.";

        v_flex()
            .w_full()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(color.border_variant)
            .bg(linear_gradient(
                360.,
                linear_color_stop(color.panel_background, 1.0),
                linear_color_stop(color.editor_background, 0.45),
            ))
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::MavAssistant)
                            .color(Color::Muted)
                            .size(IconSize::Small),
                    )
                    .child(Label::new("Collaborate with Agents")),
            )
            .child(
                Label::new(description)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .mb_2(),
            )
            .child(
                Button::new("open-agent", "Open Agent Panel")
                    .full_width()
                    .tab_index(tab_index as isize)
                    .style(ButtonStyle::Outlined)
                    .key_binding(
                        KeyBinding::for_action_in(&ToggleFocus, &self.focus_handle, cx)
                            .size(rems_from_px(12.)),
                    )
                    .on_click(move |_, window, cx| {
                        focus.dispatch_action(&ToggleSidebar, window, cx);
                        focus.dispatch_action(&ToggleFocus, window, cx);
                    }),
            )
    }

    fn render_recent_project_section(
        &self,
        recent_projects: Vec<impl IntoElement>,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .child(SectionHeader::new("Recent Projects"))
            .children(recent_projects)
    }

    fn render_recent_project(
        &self,
        project_index: usize,
        tab_index: usize,
        location: &SerializedWorkspaceLocation,
        paths: &PathList,
    ) -> impl IntoElement {
        let name = project_name(paths);

        let (icon, title) = match location {
            SerializedWorkspaceLocation::Local => (IconName::Folder, name),
            SerializedWorkspaceLocation::Remote(_) => (IconName::Server, name),
        };

        SectionButton::new(
            title,
            icon,
            &OpenRecentProject {
                index: project_index,
            },
            tab_index,
            self.focus_handle.clone(),
        )
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (first_section, second_section) = CONTENT;
        let first_section_entries = first_section.entries.len();
        let mut next_tab_index = first_section_entries + second_section.entries.len();

        let ai_enabled = AgentSettings::get_global(cx).enabled(cx);

        let recent_projects = self
            .recent_workspaces
            .as_ref()
            .into_iter()
            .flatten()
            .take(5)
            .enumerate()
            .map(|(index, workspace)| {
                self.render_recent_project(
                    index,
                    first_section_entries + index,
                    &workspace.location,
                    &workspace.identity_paths,
                )
            })
            .collect::<Vec<_>>();

        let showing_recent_projects =
            self.fallback_to_recent_projects && !recent_projects.is_empty();
        let second_section = if showing_recent_projects {
            self.render_recent_project_section(recent_projects)
                .into_any_element()
        } else {
            second_section
                .render(first_section_entries, &self.focus_handle)
                .into_any_element()
        };

        let welcome_label = if self.fallback_to_recent_projects {
            "Welcome back to Mav"
        } else {
            "Welcome to Mav"
        };

        h_flex()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::open_recent_project))
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .justify_center()
            .child(
                v_flex()
                    .id("welcome-content")
                    .p_8()
                    .max_w_128()
                    .size_full()
                    .gap_6()
                    .justify_center()
                    .overflow_y_scroll()
                    .child(
                        h_flex()
                            .w_full()
                            .justify_center()
                            .mb_4()
                            .gap_4()
                            .child(Vector::square(VectorName::MavLogo, rems_from_px(45.)))
                            .child(
                                v_flex().child(Headline::new(welcome_label)).child(
                                    Label::new("The editor for what's next")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted)
                                        .italic(),
                                ),
                            ),
                    )
                    .child(first_section.render(Default::default(), &self.focus_handle))
                    .child(second_section)
                    .when(ai_enabled && !showing_recent_projects, |this| {
                        let agent_tab_index = next_tab_index;
                        next_tab_index += 1;
                        this.child(self.render_agent_card(agent_tab_index, cx))
                    })
                    .when(!self.fallback_to_recent_projects, |this| {
                        this.child(
                            v_flex().gap_4().child(Divider::horizontal()).child(
                                Button::new("welcome-exit", "Return to Onboarding")
                                    .tab_index(next_tab_index as isize)
                                    .full_width()
                                    .label_size(LabelSize::XSmall)
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(OpenOnboarding.boxed_clone(), cx);
                                    }),
                            ),
                        )
                    }),
            )
    }
}
