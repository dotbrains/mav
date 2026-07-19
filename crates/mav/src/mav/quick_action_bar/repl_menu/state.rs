use gpui::{App, Entity, SharedString};
use repl::{ExecutionState, Kernel, KernelStatus, Session};
use ui::{Color, IconName, Indicator};

pub(super) struct ReplMenuState {
    pub(super) tooltip: SharedString,
    pub(super) icon: IconName,
    pub(super) icon_color: Color,
    pub(super) icon_is_animating: bool,
    pub(super) popover_disabled: bool,
    pub(super) indicator: Option<Indicator>,

    pub(super) status: KernelStatus,
    pub(super) kernel_name: SharedString,
    pub(super) kernel_language: SharedString,
}

pub(super) fn session_state(session: Entity<Session>, cx: &mut App) -> ReplMenuState {
    let session = session.read(cx);

    let kernel_name = session.kernel_specification.name();
    let kernel_language: SharedString = session.kernel_specification.language();

    let fill_fields = || {
        ReplMenuState {
            tooltip: "Nothing running".into(),
            icon: IconName::ReplNeutral,
            icon_color: Color::Default,
            icon_is_animating: false,
            popover_disabled: false,
            indicator: None,
            kernel_name: kernel_name.clone(),
            kernel_language: kernel_language.clone(),
            // TODO: Technically not shutdown, but indeterminate.
            status: KernelStatus::Shutdown,
        }
    };

    let transitional =
        |tooltip: SharedString, animating: bool, popover_disabled: bool| ReplMenuState {
            tooltip,
            icon_is_animating: animating,
            popover_disabled,
            icon_color: Color::Muted,
            indicator: Some(Indicator::dot().color(Color::Muted)),
            status: session.kernel.status(),
            ..fill_fields()
        };

    let starting = || transitional(format!("{} is starting", kernel_name).into(), true, true);
    let restarting = || transitional(format!("Restarting {}", kernel_name).into(), true, true);
    let shutting_down = || {
        transitional(
            format!("{} is shutting down", kernel_name).into(),
            false,
            true,
        )
    };
    let auto_restarting = || {
        transitional(
            format!("Auto-restarting {}", kernel_name).into(),
            true,
            true,
        )
    };
    let unknown = || transitional(format!("{} state unknown", kernel_name).into(), false, true);
    let other = |state: &str| {
        transitional(
            format!("{} state: {}", kernel_name, state).into(),
            false,
            true,
        )
    };

    let shutdown = || ReplMenuState {
        tooltip: "Nothing running".into(),
        icon: IconName::ReplNeutral,
        icon_color: Color::Default,
        icon_is_animating: false,
        popover_disabled: false,
        indicator: None,
        status: KernelStatus::Shutdown,
        ..fill_fields()
    };

    match &session.kernel {
        Kernel::Restarting => restarting(),
        Kernel::RunningKernel(kernel) => match &kernel.execution_state() {
            ExecutionState::Idle => ReplMenuState {
                tooltip: format!("Run code on {} ({})", kernel_name, kernel_language).into(),
                indicator: Some(Indicator::dot().color(Color::Success)),
                status: session.kernel.status(),
                ..fill_fields()
            },
            ExecutionState::Busy => ReplMenuState {
                tooltip: format!("Interrupt {} ({})", kernel_name, kernel_language).into(),
                icon_is_animating: true,
                popover_disabled: false,
                indicator: None,
                status: session.kernel.status(),
                ..fill_fields()
            },
            ExecutionState::Unknown => unknown(),
            ExecutionState::Starting => starting(),
            ExecutionState::Restarting => restarting(),
            ExecutionState::Terminating => shutting_down(),
            ExecutionState::AutoRestarting => auto_restarting(),
            ExecutionState::Dead => shutdown(),
            ExecutionState::Other(state) => other(state),
        },
        Kernel::StartingKernel(_) => starting(),
        Kernel::ErroredLaunch(e) => ReplMenuState {
            tooltip: format!("Error with kernel {}: {}", kernel_name, e).into(),
            popover_disabled: false,
            indicator: Some(Indicator::dot().color(Color::Error)),
            status: session.kernel.status(),
            ..fill_fields()
        },
        Kernel::ShuttingDown => shutting_down(),
        Kernel::Shutdown => shutdown(),
    }
}
