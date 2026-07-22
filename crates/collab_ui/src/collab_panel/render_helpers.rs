use super::*;

pub(super) fn render_tree_branch(
    is_last: bool,
    overdraw: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let rem_size = window.rem_size();
    let line_height = window.text_style().line_height_in_pixels(rem_size);
    let thickness = px(1.);
    let color = cx.theme().colors().icon_disabled;

    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let start_x = (bounds.left() + bounds.right() - thickness) / 2.;
            let start_y = (bounds.top() + bounds.bottom() - thickness) / 2.;
            let right = bounds.right();
            let top = bounds.top();

            window.paint_quad(fill(
                Bounds::from_corners(
                    point(start_x, top),
                    point(
                        start_x + thickness,
                        if is_last {
                            start_y
                        } else {
                            bounds.bottom() + if overdraw { px(1.) } else { px(0.) }
                        },
                    ),
                ),
                color,
            ));
            window.paint_quad(fill(
                Bounds::from_corners(point(start_x, start_y), point(right, start_y + thickness)),
                color,
            ));
        },
    )
    .w(rem_size)
    .h(line_height - px(2.))
}

pub(super) fn render_participant_name_and_handle(user: &User) -> impl IntoElement {
    Label::new(if let Some(ref display_name) = user.name {
        format!("{display_name} ({})", user.username)
    } else {
        user.username.to_string()
    })
}

pub(super) struct DraggedChannelView {
    pub(super) channel: Channel,
    pub(super) width: Pixels,
}

impl Render for DraggedChannelView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = ThemeSettings::get_global(cx).ui_font.family.clone();
        h_flex()
            .font_family(ui_font)
            .bg(cx.theme().colors().background)
            .w(self.width)
            .p_1()
            .gap_1()
            .child(
                Icon::new(
                    if self.channel.visibility == proto::ChannelVisibility::Public {
                        IconName::Public
                    } else {
                        IconName::Hash
                    },
                )
                .size(IconSize::Small)
                .color(Color::Muted),
            )
            .child(Label::new(self.channel.name.clone()))
    }
}

pub(super) struct JoinChannelTooltip {
    pub(super) channel_store: Entity<ChannelStore>,
    pub(super) channel_id: ChannelId,
    #[allow(unused)]
    pub(super) has_notes_notification: bool,
}

impl Render for JoinChannelTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_container(cx, |container, cx| {
            let participants = self
                .channel_store
                .read(cx)
                .channel_participants(self.channel_id);

            container
                .child(Label::new("Join Channel"))
                .children(participants.iter().map(|participant| {
                    h_flex()
                        .gap_2()
                        .child(Avatar::new(participant.avatar_uri.clone()))
                        .child(render_participant_name_and_handle(participant))
                }))
        })
    }
}
