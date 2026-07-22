use std::time::{Duration, Instant};

use gpui::{Context, Task};

use super::MessageNotification;

const FADE_OUT_DURATION: Duration = Duration::from_secs(2);
const FADE_TO_FULL_OPACITY_DURATION: Duration = Duration::from_millis(200);

pub(super) struct AutoHideState {
    remaining_dismiss_duration: Duration,
    timer_started: Option<Instant>,
    hovered: bool,
    fade: Option<AutoHideFade>,
    task: Option<Task<()>>,
}

enum AutoHideFade {
    FadingOut {
        started_at: Instant,
    },
    FadingIn {
        started_at: Instant,
        start_opacity: f32,
    },
}

impl AutoHideState {
    pub(super) fn new(duration: Duration, cx: &mut Context<MessageNotification>) -> Self {
        let mut this = Self {
            remaining_dismiss_duration: duration,
            timer_started: None,
            hovered: false,
            fade: None,
            task: None,
        };
        this.schedule(cx);
        this
    }

    fn schedule(&mut self, cx: &mut Context<MessageNotification>) {
        if self.task.is_some() || self.hovered {
            return;
        }

        let duration = self.remaining_dismiss_duration;
        self.timer_started = Some(Instant::now());
        self.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            if let Err(error) = this.update(cx, |this, cx| {
                if let Some(auto_hide) = this.auto_hide.as_mut() {
                    auto_hide.finish_timer();
                    if !auto_hide.hovered {
                        auto_hide.start_fading_out();
                        cx.notify();
                    }
                }
            }) {
                log::error!("failed to update auto-hiding notification: {error:?}");
            }
        }));
    }

    pub(super) fn set_hovered(&mut self, hovered: bool, cx: &mut Context<MessageNotification>) {
        if self.hovered == hovered {
            return;
        }

        self.hovered = hovered;
        if hovered {
            self.remaining_dismiss_duration = self.remaining_dismiss_duration();
            self.timer_started = None;
            self.task.take();

            if matches!(self.fade, Some(AutoHideFade::FadingOut { .. })) {
                let start_opacity = self.opacity();
                self.fade = Some(AutoHideFade::FadingIn {
                    started_at: Instant::now(),
                    start_opacity,
                });
            }
        } else {
            if matches!(self.fade, Some(AutoHideFade::FadingIn { .. })) {
                self.fade = None;
            }
            self.schedule(cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_animation(&mut self) -> bool {
        match self.fade {
            Some(AutoHideFade::FadingOut { started_at })
                if started_at.elapsed() >= FADE_OUT_DURATION =>
            {
                true
            }
            Some(AutoHideFade::FadingIn { started_at, .. })
                if started_at.elapsed() >= FADE_TO_FULL_OPACITY_DURATION =>
            {
                self.fade = None;
                false
            }
            _ => false,
        }
    }

    pub(super) fn needs_animation_frame(&self) -> bool {
        self.fade.is_some()
    }

    pub(super) fn opacity(&self) -> f32 {
        match self.fade {
            Some(AutoHideFade::FadingOut { started_at }) => {
                1.0 - duration_progress(started_at.elapsed(), FADE_OUT_DURATION)
            }
            Some(AutoHideFade::FadingIn {
                started_at,
                start_opacity,
            }) => {
                let progress =
                    duration_progress(started_at.elapsed(), FADE_TO_FULL_OPACITY_DURATION);
                start_opacity + (1.0 - start_opacity) * progress
            }
            None => 1.0,
        }
    }

    fn finish_timer(&mut self) {
        self.task.take();
        self.timer_started = None;
        self.remaining_dismiss_duration = Duration::ZERO;
    }

    fn start_fading_out(&mut self) {
        self.fade = Some(AutoHideFade::FadingOut {
            started_at: Instant::now(),
        });
    }

    fn remaining_dismiss_duration(&self) -> Duration {
        self.timer_started
            .map_or(self.remaining_dismiss_duration, |timer_started| {
                self.remaining_dismiss_duration
                    .saturating_sub(timer_started.elapsed())
            })
    }
}

fn duration_progress(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0)
    }
}
