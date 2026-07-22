use super::*;

#[derive(Clone, Default)]
pub(super) struct AcpConnectionDefaults {
    mode: Rc<RefCell<Option<acp::SessionModeId>>>,
    config_options: Rc<RefCell<HashMap<String, AgentConfigOptionValue>>>,
}

impl AcpConnectionDefaults {
    pub(super) fn new(
        mode: Option<acp::SessionModeId>,
        config_options: HashMap<String, AgentConfigOptionValue>,
    ) -> Self {
        Self {
            mode: Rc::new(RefCell::new(mode)),
            config_options: Rc::new(RefCell::new(config_options)),
        }
    }

    pub(super) fn mode(&self) -> Option<acp::SessionModeId> {
        self.mode.borrow().clone()
    }

    pub(super) fn config_option(&self, config_id: &str) -> Option<AgentConfigOptionValue> {
        self.config_options.borrow().get(config_id).cloned()
    }

    pub(super) fn set(
        &self,
        mode: Option<acp::SessionModeId>,
        config_options: HashMap<String, AgentConfigOptionValue>,
    ) {
        *self.mode.borrow_mut() = mode;
        *self.config_options.borrow_mut() = config_options;
    }

    pub(super) fn refresh_from_settings(&self, agent_id: &AgentId, cx: &App) {
        let Some(settings_store) = cx.try_global::<SettingsStore>() else {
            self.set(None, HashMap::default());
            return;
        };
        let settings = settings_store.get::<AllAgentServersSettings>(None);
        let Some(agent_settings) = settings.get(agent_id.as_ref()) else {
            self.set(None, HashMap::default());
            return;
        };

        let default_config_options = match agent_settings {
            CustomAgentServerSettings::Custom {
                default_config_options,
                ..
            }
            | CustomAgentServerSettings::Registry {
                default_config_options,
                ..
            } => default_config_options.clone(),
        };
        self.set(
            agent_settings.default_mode().map(acp::SessionModeId::new),
            default_config_options,
        );
    }

    pub(super) fn observe_settings(&self, agent_id: AgentId, cx: &mut App) -> Subscription {
        if cx.try_global::<SettingsStore>().is_none() {
            return Subscription::new(|| {});
        }

        self.refresh_from_settings(&agent_id, cx);
        let defaults = self.clone();
        cx.observe_global::<SettingsStore>(move |cx| {
            defaults.refresh_from_settings(&agent_id, cx);
        })
    }
}
