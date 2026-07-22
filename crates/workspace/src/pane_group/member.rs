use super::sizing::split_flexes_for_inserted_size;
use super::*;

#[derive(Debug, Clone)]
pub enum Member {
    Axis(PaneAxis),
    Pane(Entity<Pane>),
}

impl Member {
    pub fn mark_positions(
        &mut self,
        in_center_group: bool,
        top_left_pane: Option<&Entity<Pane>>,
        cx: &mut App,
    ) {
        match self {
            Member::Axis(pane_axis) => {
                for member in pane_axis.members.iter_mut() {
                    member.mark_positions(in_center_group, top_left_pane, cx);
                }
            }
            Member::Pane(entity) => entity.update(cx, |pane, cx| {
                pane.in_center_group = in_center_group;
                pane.set_reserve_traffic_light_space(
                    in_center_group && top_left_pane.is_some_and(|pane| pane == entity),
                    cx,
                );
            }),
        }
    }

    pub(super) fn full_height_column_count(&self) -> usize {
        match self {
            Member::Pane(_) => 1,
            Member::Axis(axis) => axis.full_height_column_count(),
        }
    }

    pub(super) fn is_visible(&self, cx: &App) -> bool {
        match self {
            Member::Axis(axis) => axis.members.iter().any(|member| member.is_visible(cx)),
            Member::Pane(pane) => pane.read(cx).is_visible(),
        }
    }

    pub(super) fn clear_bounding_boxes(&self) {
        match self {
            Member::Axis(axis) => {
                axis.bounding_boxes.lock().fill(None);
                for member in &axis.members {
                    member.clear_bounding_boxes();
                }
            }
            Member::Pane(_) => {}
        }
    }
}

#[derive(Clone, Copy)]
pub struct PaneRenderContext<'a> {
    pub project: &'a Entity<Project>,
    pub follower_states: &'a HashMap<CollaboratorId, FollowerState>,
    pub active_call: Option<&'a dyn AnyActiveCall>,
    pub active_pane: &'a Entity<Pane>,
    pub app_state: &'a Arc<AppState>,
    pub workspace: &'a WeakEntity<Workspace>,
}

#[derive(Default)]
pub struct LeaderDecoration {
    pub(super) border: Option<Hsla>,
    pub(super) status_box: Option<AnyElement>,
}

pub trait PaneLeaderDecorator {
    fn decorate(&self, pane: &Entity<Pane>, cx: &App) -> LeaderDecoration;
    fn active_pane(&self) -> &Entity<Pane>;
    fn workspace(&self) -> &WeakEntity<Workspace>;
}

pub struct ActivePaneDecorator<'a> {
    active_pane: &'a Entity<Pane>,
    workspace: &'a WeakEntity<Workspace>,
}

impl<'a> ActivePaneDecorator<'a> {
    pub fn new(active_pane: &'a Entity<Pane>, workspace: &'a WeakEntity<Workspace>) -> Self {
        Self {
            active_pane,
            workspace,
        }
    }
}

impl PaneLeaderDecorator for ActivePaneDecorator<'_> {
    fn decorate(&self, _: &Entity<Pane>, _: &App) -> LeaderDecoration {
        LeaderDecoration::default()
    }
    fn active_pane(&self) -> &Entity<Pane> {
        self.active_pane
    }

    fn workspace(&self) -> &WeakEntity<Workspace> {
        self.workspace
    }
}

impl PaneLeaderDecorator for PaneRenderContext<'_> {
    fn decorate(&self, pane: &Entity<Pane>, cx: &App) -> LeaderDecoration {
        let follower_state = self.follower_states.iter().find_map(|(leader_id, state)| {
            if state.center_pane == *pane {
                Some((*leader_id, state))
            } else {
                None
            }
        });
        let Some((leader_id, follower_state)) = follower_state else {
            return LeaderDecoration::default();
        };

        let mut leader_color;
        let status_box;
        match leader_id {
            CollaboratorId::PeerId(peer_id) => {
                let Some(leader) = self
                    .active_call
                    .as_ref()
                    .and_then(|call| call.remote_participant_for_peer_id(peer_id, cx))
                else {
                    return LeaderDecoration::default();
                };

                let is_in_unshared_view = follower_state.active_view_id.is_some_and(|view_id| {
                    !follower_state
                        .items_by_leader_view_id
                        .contains_key(&view_id)
                });

                let mut leader_join_data = None;
                let leader_status_box = match leader.location {
                    ParticipantLocation::SharedProject {
                        project_id: leader_project_id,
                    } => {
                        if Some(leader_project_id) == self.project.read(cx).remote_id() {
                            is_in_unshared_view.then(|| {
                                Label::new(format!(
                                    "{} is in an unshared pane",
                                    leader.user.username
                                ))
                            })
                        } else {
                            leader_join_data = Some((leader_project_id, leader.user.legacy_id));
                            Some(Label::new(format!(
                                "Follow {} to their active project",
                                leader.user.username,
                            )))
                        }
                    }
                    ParticipantLocation::UnsharedProject => Some(Label::new(format!(
                        "{} is viewing an unshared Mav project",
                        leader.user.username
                    ))),
                    ParticipantLocation::External => Some(Label::new(format!(
                        "{} is viewing a window outside of Mav",
                        leader.user.username
                    ))),
                };
                status_box = leader_status_box.map(|status| {
                    div()
                        .absolute()
                        .w_96()
                        .bottom_3()
                        .right_3()
                        .elevation_2(cx)
                        .p_1()
                        .child(status)
                        .when_some(
                            leader_join_data,
                            |this, (leader_project_id, leader_user_id)| {
                                let app_state = self.app_state.clone();
                                this.cursor_pointer().on_mouse_down(
                                    MouseButton::Left,
                                    move |_, window, cx| {
                                        crate::join_in_room_project(
                                            leader_project_id,
                                            leader_user_id,
                                            app_state.clone(),
                                            cx,
                                        )
                                        .detach_and_prompt_err(
                                            "Failed to join project",
                                            window,
                                            cx,
                                            |error, _, _| Some(format!("{error:#}")),
                                        );
                                    },
                                )
                            },
                        )
                        .into_any_element()
                });
                leader_color = cx
                    .theme()
                    .players()
                    .color_for_participant(leader.participant_index.0)
                    .cursor;
            }
            CollaboratorId::Agent => {
                status_box = None;
                leader_color = cx.theme().players().agent().cursor;
            }
        }

        let is_in_panel = follower_state.dock_pane.is_some();
        if is_in_panel {
            leader_color.fade_out(0.75);
        } else {
            leader_color.fade_out(0.3);
        }

        LeaderDecoration {
            status_box,
            border: Some(leader_color),
        }
    }

    fn active_pane(&self) -> &Entity<Pane> {
        self.active_pane
    }

    fn workspace(&self) -> &WeakEntity<Workspace> {
        self.workspace
    }
}

impl Member {
    pub(super) fn new_axis_with_size_hint(
        old_pane: Entity<Pane>,
        new_pane: Entity<Pane>,
        direction: SplitDirection,
        size_hint: Option<SplitSizeHint>,
        available_size: Option<Pixels>,
    ) -> Self {
        use Axis::*;
        use SplitDirection::*;

        let axis = match direction {
            Up | Down => Vertical,
            Left | Right => Horizontal,
        };

        let members = match direction {
            Up | Left => vec![Member::Pane(new_pane), Member::Pane(old_pane)],
            Down | Right => vec![Member::Pane(old_pane), Member::Pane(new_pane)],
        };

        let flexes = size_hint
            .filter(|_| axis == Axis::Horizontal)
            .and_then(|hint| Some((hint, hint.available_size.or(available_size)?)))
            .and_then(|(hint, available_size)| {
                split_flexes_for_inserted_size(
                    available_size,
                    hint.inserted_size,
                    direction.increasing(),
                )
            });

        match flexes {
            Some(flexes) => Member::Axis(PaneAxis::load(axis, members, Some(flexes))),
            None => Member::Axis(PaneAxis::new(axis, members)),
        }
    }

    pub(super) fn first_pane(&self) -> Entity<Pane> {
        match self {
            Member::Axis(axis) => axis.members[0].first_pane(),
            Member::Pane(pane) => pane.clone(),
        }
    }

    pub(super) fn first_visible_pane(&self, cx: &App) -> Option<Entity<Pane>> {
        match self {
            Member::Axis(axis) => axis
                .members
                .iter()
                .find_map(|member| member.first_visible_pane(cx)),
            Member::Pane(pane) => pane.read(cx).is_visible().then(|| pane.clone()),
        }
    }

    pub(super) fn last_pane(&self) -> Entity<Pane> {
        match self {
            Member::Axis(axis) => axis.members.last().unwrap().last_pane(),
            Member::Pane(pane) => pane.clone(),
        }
    }

    pub fn render(
        &self,
        basis: usize,
        zoomed: Option<&AnyWeakView>,
        render_cx: &dyn PaneLeaderDecorator,
        window: &mut Window,
        cx: &mut App,
    ) -> PaneRenderResult {
        match self {
            Member::Pane(pane) => {
                if !pane.read(cx).is_visible() {
                    return PaneRenderResult {
                        element: div().into_any(),
                        contains_active_pane: false,
                    };
                }

                if zoomed == Some(&pane.downgrade().into()) {
                    return PaneRenderResult {
                        element: div().into_any(),
                        contains_active_pane: false,
                    };
                }

                let is_active = pane == render_cx.active_pane();

                PaneRenderResult {
                    element: render_pane_card(pane.clone(), render_cx, cx),
                    contains_active_pane: is_active,
                }
            }
            Member::Axis(axis) => axis.render(basis + 1, zoomed, render_cx, window, cx),
        }
    }

    pub(super) fn collect_panes<'a>(&'a self, panes: &mut Vec<&'a Entity<Pane>>) {
        match self {
            Member::Axis(axis) => {
                for member in &axis.members {
                    member.collect_panes(panes);
                }
            }
            Member::Pane(pane) => panes.push(pane),
        }
    }

    pub(super) fn contains_pane(&self, pane: &Entity<Pane>) -> bool {
        match self {
            Member::Axis(axis) => axis.members.iter().any(|member| member.contains_pane(pane)),
            Member::Pane(candidate) => candidate == pane,
        }
    }

    pub(super) fn collect_pane_bounds(&self, panes: &mut Vec<(Entity<Pane>, Bounds<Pixels>)>) {
        match self {
            Member::Axis(axis) => axis.collect_pane_bounds(panes),
            Member::Pane(_) => {}
        }
    }

    pub(super) fn invert_pane_axies(&mut self) {
        match self {
            Self::Axis(axis) => {
                axis.axis = axis.axis.invert();
                for member in axis.members.iter_mut() {
                    member.invert_pane_axies();
                }
            }
            Self::Pane(_) => {}
        }
    }

    pub(super) fn normalize_same_axis(&mut self) {
        if let Self::Axis(axis) = self {
            axis.normalize_same_axis();
        }
    }
}
