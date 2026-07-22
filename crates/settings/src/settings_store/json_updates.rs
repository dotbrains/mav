use super::*;

impl SettingsStore {
    pub fn new_text_for_update(
        &self,
        old_text: String,
        update: impl FnOnce(&mut SettingsContent),
    ) -> Result<String> {
        let edits = self.edits_for_update(&old_text, update)?;
        let mut new_text = old_text;
        for (range, replacement) in edits.into_iter() {
            new_text.replace_range(range, &replacement);
        }
        Ok(new_text)
    }

    pub fn get_vscode_edits(&self, old_text: String, vscode: &VsCodeSettings) -> Result<String> {
        self.new_text_for_update(old_text, |content| {
            content.merge_from(&vscode.settings_content())
        })
    }

    /// Updates the value of a setting in a JSON file, returning a list
    /// of edits to apply to the JSON file.
    pub fn edits_for_update(
        &self,
        text: &str,
        update: impl FnOnce(&mut SettingsContent),
    ) -> Result<Vec<(Range<usize>, String)>> {
        let old_content = if text.trim().is_empty() {
            UserSettingsContent::default()
        } else {
            let (old_content, parse_status) = UserSettingsContent::parse_json(text);
            if let ParseStatus::Failed { error } = &parse_status {
                log::error!("Failed to parse settings for update: {error}");
            }
            old_content
                .context("Settings file could not be parsed. Fix syntax errors before updating.")?
        };
        let mut new_content = old_content.clone();
        update(&mut new_content.content);

        let old_value = serde_json::to_value(&old_content).unwrap();
        let new_value = serde_json::to_value(new_content).unwrap();

        let mut key_path = Vec::new();
        let mut edits = Vec::new();
        let tab_size = infer_json_indent_size(&text);
        let mut text = text.to_string();
        update_value_in_json_text(
            &mut text,
            &mut key_path,
            tab_size,
            &old_value,
            &new_value,
            &mut edits,
        );
        Ok(edits)
    }

    /// Mutates the default settings in place and recomputes all setting values.
    pub fn update_default_settings(
        &mut self,
        cx: &mut App,
        update: impl FnOnce(&mut SettingsContent),
    ) {
        let default_settings = Rc::make_mut(&mut self.default_settings);
        update(default_settings);
        self.recompute_values(None, cx);
    }

    /// Sets the default settings via a JSON string.
    ///
    /// The string should contain a JSON object with a default value for every setting.
    pub fn set_default_settings(
        &mut self,
        default_settings_content: &str,
        cx: &mut App,
    ) -> Result<()> {
        self.default_settings = Self::parse_default_settings(default_settings_content)?.into();
        self.recompute_values(None, cx);
        Ok(())
    }

    /// Parses the default settings JSON and folds any `dev`/`nightly`/`preview`/`stable`
    /// release-channel overrides and `macos`/`linux`/`windows` platform overrides into
    /// the returned [`SettingsContent`].
    ///
    /// Unlike user settings, default settings are used directly as the base for all
    /// merges, so overrides must be resolved up front.
    pub(super) fn parse_default_settings(default_settings: &str) -> Result<SettingsContent> {
        let parsed = UserSettingsContent::parse_json_with_comments(default_settings)?;
        let mut merged = (*parsed.content).clone();
        merged.merge_from_option(parsed.for_release_channel());
        merged.merge_from_option(parsed.for_os());
        Ok(merged)
    }
}
