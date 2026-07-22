use super::*;

impl SettingsStore {
    pub(super) fn recompute_values(
        &mut self,
        changed_local_path: Option<(WorktreeId, &RelPath)>,
        cx: &mut App,
    ) {
        // Reload the global and local values for every setting.
        let mut project_settings_stack = Vec::<SettingsContent>::new();
        let mut paths_stack = Vec::<Option<(WorktreeId, &RelPath)>>::new();

        if changed_local_path.is_none() {
            let mut merged = self.default_settings.as_ref().clone();
            merged.merge_from_option(self.extension_settings.as_deref());
            merged.merge_from_option(self.global_settings.as_deref());
            if let Some(user_settings) = self.user_settings.as_ref() {
                let active_profile = user_settings.for_profile(cx);
                let should_merge_user_settings =
                    active_profile.is_none_or(|profile| profile.base == ProfileBase::User);

                if should_merge_user_settings {
                    merged.merge_from(&user_settings.content);
                    merged.merge_from_option(user_settings.for_release_channel());
                    merged.merge_from_option(user_settings.for_os());
                }

                if let Some(profile) = active_profile {
                    merged.merge_from(&profile.settings);
                }
            }
            merged.merge_from_option(self.server_settings.as_deref());

            // Merge `disable_ai` from all project/local settings into the global value.
            // Since `SaturatingBool` uses OR logic, if any project has `disable_ai: true`,
            // the global value will be true. This allows project-level `disable_ai` to
            // affect the global setting used by UI elements without file context.
            for local_settings in self.local_settings.values() {
                merged
                    .project
                    .disable_ai
                    .merge_from(&local_settings.project.disable_ai);
            }

            self.merged_settings = Rc::new(merged);

            for setting_value in self.setting_values.values_mut() {
                let value = setting_value.from_settings(&self.merged_settings);
                setting_value.set_global_value(value);
            }
        } else {
            // When only a local path changed, we still need to recompute the global
            // `disable_ai` value since it depends on all local settings.
            let mut merged = (*self.merged_settings).clone();
            // Reset disable_ai to compute fresh from base settings
            merged.project.disable_ai = self.default_settings.project.disable_ai;
            if let Some(global) = &self.global_settings {
                merged
                    .project
                    .disable_ai
                    .merge_from(&global.project.disable_ai);
            }
            if let Some(user) = &self.user_settings {
                merged
                    .project
                    .disable_ai
                    .merge_from(&user.content.project.disable_ai);
            }
            if let Some(server) = &self.server_settings {
                merged
                    .project
                    .disable_ai
                    .merge_from(&server.project.disable_ai);
            }
            for local_settings in self.local_settings.values() {
                merged
                    .project
                    .disable_ai
                    .merge_from(&local_settings.project.disable_ai);
            }
            self.merged_settings = Rc::new(merged);

            for setting_value in self.setting_values.values_mut() {
                let value = setting_value.from_settings(&self.merged_settings);
                setting_value.set_global_value(value);
            }
        }

        for ((root_id, directory_path), local_settings) in &self.local_settings {
            // Build a stack of all of the local values for that setting.
            while let Some(prev_entry) = paths_stack.last() {
                if let Some((prev_root_id, prev_path)) = prev_entry
                    && (root_id != prev_root_id || !directory_path.starts_with(prev_path))
                {
                    paths_stack.pop();
                    project_settings_stack.pop();
                    continue;
                }
                break;
            }

            paths_stack.push(Some((*root_id, directory_path.as_ref())));
            let mut merged_local_settings = if let Some(deepest) = project_settings_stack.last() {
                (*deepest).clone()
            } else {
                self.merged_settings.as_ref().clone()
            };
            merged_local_settings.merge_from(local_settings);

            project_settings_stack.push(merged_local_settings);

            // If a local settings file changed, then avoid recomputing local
            // settings for any path outside of that directory.
            if changed_local_path.is_some_and(|(changed_root_id, changed_local_path)| {
                *root_id != changed_root_id || !directory_path.starts_with(changed_local_path)
            }) {
                continue;
            }

            for setting_value in self.setting_values.values_mut() {
                let value = setting_value.from_settings(&project_settings_stack.last().unwrap());
                setting_value.set_local_value(*root_id, directory_path.clone(), value);
            }
        }
    }
}
