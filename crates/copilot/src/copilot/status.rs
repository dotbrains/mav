use super::*;

impl Copilot {
    pub fn status(&self) -> Status {
        match &self.server {
            CopilotServer::Starting { task } => Status::Starting { task: task.clone() },
            CopilotServer::Disabled => Status::Disabled,
            CopilotServer::Error(error) => Status::Error(error.clone()),
            CopilotServer::Running(RunningCopilotServer { sign_in_status, .. }) => {
                match sign_in_status {
                    SignInStatus::Authorized => Status::Authorized,
                    SignInStatus::Unauthorized => Status::Unauthorized,
                    SignInStatus::SigningIn { prompt, .. } => Status::SigningIn {
                        prompt: prompt.clone(),
                    },
                    SignInStatus::SignedOut {
                        awaiting_signing_in,
                    } => Status::SignedOut {
                        awaiting_signing_in: *awaiting_signing_in,
                    },
                }
            }
        }
    }

    pub fn update_sign_in_status(
        &mut self,
        lsp_status: request::SignInStatus,
        cx: &mut Context<Self>,
    ) {
        self.buffers.retain(|buffer| buffer.is_upgradable());

        if let Ok(server) = self.server.as_running() {
            match lsp_status {
                request::SignInStatus::Ok { user: Some(_) }
                | request::SignInStatus::MaybeOk { .. }
                | request::SignInStatus::AlreadySignedIn { .. } => {
                    server.sign_in_status = SignInStatus::Authorized;
                    cx.emit(Event::CopilotAuthSignedIn);
                    notify_copilot_chat_auth_changed(cx);
                    for buffer in self.buffers.iter().cloned().collect::<Vec<_>>() {
                        if let Some(buffer) = buffer.upgrade() {
                            self.register_buffer(&buffer, cx);
                        }
                    }
                }
                request::SignInStatus::NotAuthorized { .. } => {
                    server.sign_in_status = SignInStatus::Unauthorized;
                    for buffer in self.buffers.iter().cloned().collect::<Vec<_>>() {
                        self.unregister_buffer(&buffer);
                    }
                }
                request::SignInStatus::Ok { user: None } | request::SignInStatus::NotSignedIn => {
                    if !matches!(server.sign_in_status, SignInStatus::SignedOut { .. }) {
                        server.sign_in_status = SignInStatus::SignedOut {
                            awaiting_signing_in: false,
                        };
                    }
                    cx.emit(Event::CopilotAuthSignedOut);
                    notify_copilot_chat_auth_changed(cx);
                    for buffer in self.buffers.iter().cloned().collect::<Vec<_>>() {
                        self.unregister_buffer(&buffer);
                    }
                }
            }

            cx.notify();
        }
    }

    pub(super) fn update_action_visibilities(&self, cx: &mut App) {
        let signed_in_actions = [
            TypeId::of::<Suggest>(),
            TypeId::of::<NextSuggestion>(),
            TypeId::of::<PreviousSuggestion>(),
            TypeId::of::<Reinstall>(),
        ];
        let auth_actions = [TypeId::of::<SignOut>()];
        let no_auth_actions = [TypeId::of::<SignIn>()];
        let status = self.status();

        let is_ai_disabled = DisableAiSettings::get_global(cx).disable_ai;
        let filter = CommandPaletteFilter::global_mut(cx);

        if is_ai_disabled {
            filter.hide_action_types(&signed_in_actions);
            filter.hide_action_types(&auth_actions);
            filter.hide_action_types(&no_auth_actions);
        } else {
            match status {
                Status::Disabled => {
                    filter.hide_action_types(&signed_in_actions);
                    filter.hide_action_types(&auth_actions);
                    filter.hide_action_types(&no_auth_actions);
                }
                Status::Authorized => {
                    filter.hide_action_types(&no_auth_actions);
                    filter.show_action_types(signed_in_actions.iter().chain(&auth_actions));
                }
                _ => {
                    filter.hide_action_types(&signed_in_actions);
                    filter.hide_action_types(&auth_actions);
                    filter.show_action_types(&no_auth_actions);
                }
            }
        }
    }
}
