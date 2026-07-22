use super::*;

impl KeymapFile {
    pub fn action_schema_generator() -> schemars::SchemaGenerator {
        schemars::generate::SchemaSettings::draft2019_09()
            .with_transform(AllowTrailingCommas)
            .into_generator()
    }

    pub fn generate_json_schema_for_registered_actions(cx: &mut App) -> Value {
        // instead of using DefaultDenyUnknownFields, actions typically use
        // `#[serde(deny_unknown_fields)]` so that these cases are reported as parse failures. This
        // is because the rest of the keymap will still load in these cases, whereas other settings
        // files would not.
        let mut generator = Self::action_schema_generator();

        let action_schemas = cx.action_schemas(&mut generator);
        let action_documentation = cx.action_documentation();
        let deprecations = cx.deprecated_actions_to_preferred_actions();
        let deprecation_messages = cx.action_deprecation_messages();
        KeymapFile::generate_json_schema(
            generator,
            action_schemas,
            action_documentation,
            deprecations,
            deprecation_messages,
        )
    }

    pub fn generate_json_schema_from_inventory() -> Value {
        let mut generator = Self::action_schema_generator();

        let mut action_schemas = Vec::new();
        let mut documentation = HashMap::default();
        let mut deprecations = HashMap::default();
        let mut deprecation_messages = HashMap::default();

        for action_data in generate_list_of_all_registered_actions() {
            let schema = (action_data.json_schema)(&mut generator);
            action_schemas.push((action_data.name, schema));

            if let Some(doc) = action_data.documentation {
                documentation.insert(action_data.name, doc);
            }
            if let Some(msg) = action_data.deprecation_message {
                deprecation_messages.insert(action_data.name, msg);
            }
            for &alias in action_data.deprecated_aliases {
                deprecations.insert(alias, action_data.name);

                let alias_schema = (action_data.json_schema)(&mut generator);
                action_schemas.push((alias, alias_schema));
            }
        }

        KeymapFile::generate_json_schema(
            generator,
            action_schemas,
            &documentation,
            &deprecations,
            &deprecation_messages,
        )
    }

    pub fn get_action_schema_by_name(
        action_name: &str,
        generator: &mut schemars::SchemaGenerator,
    ) -> Option<schemars::Schema> {
        for action_data in generate_list_of_all_registered_actions() {
            if action_data.name == action_name {
                return (action_data.json_schema)(generator);
            }
            for &alias in action_data.deprecated_aliases {
                if alias == action_name {
                    return (action_data.json_schema)(generator);
                }
            }
        }
        None
    }

    pub fn generate_json_schema<'a>(
        mut generator: schemars::SchemaGenerator,
        action_schemas: Vec<(&'a str, Option<schemars::Schema>)>,
        action_documentation: &HashMap<&'a str, &'a str>,
        deprecations: &HashMap<&'a str, &'a str>,
        deprecation_messages: &HashMap<&'a str, &'a str>,
    ) -> serde_json::Value {
        fn add_deprecation(schema: &mut schemars::Schema, message: String) {
            schema.insert(
                // deprecationMessage is not part of the JSON Schema spec, but
                // json-language-server recognizes it.
                "deprecationMessage".to_string(),
                Value::String(message),
            );
        }

        fn add_deprecation_preferred_name(schema: &mut schemars::Schema, new_name: &str) {
            add_deprecation(schema, format!("Deprecated, use {new_name}"));
        }

        fn add_description(schema: &mut schemars::Schema, description: &str) {
            schema.insert(
                "description".to_string(),
                Value::String(description.to_string()),
            );
        }

        let empty_object = json_schema!({
            "type": "object"
        });

        // This is a workaround for a json-language-server issue where it matches the first
        // alternative that matches the value's shape and uses that for documentation.
        //
        // In the case of the array validations, it would even provide an error saying that the name
        // must match the name of the first alternative.
        let mut empty_action_name = json_schema!({
            "type": "string",
            "const": ""
        });
        let no_action_message = "No action named this.";
        add_description(&mut empty_action_name, no_action_message);
        add_deprecation(&mut empty_action_name, no_action_message.to_string());
        let empty_action_name_with_input = json_schema!({
            "type": "array",
            "items": [
                empty_action_name,
                true
            ],
            "minItems": 2,
            "maxItems": 2
        });

        let mut keymap_deprecations = deprecations.clone();
        keymap_deprecations.insert(NoAction.name(), "null");
        let action_name_schema = ActionName::build_schema(
            action_schemas.iter().map(|(name, _)| *name),
            action_documentation,
            &keymap_deprecations,
            deprecation_messages,
        );

        let mut action_with_arguments_alternatives = vec![empty_action_name_with_input.clone()];
        let mut unbind_target_action_alternatives =
            vec![empty_action_name, empty_action_name_with_input];

        let mut empty_schema_action_names = vec![];
        let mut empty_schema_unbind_target_action_names = vec![];
        for (name, action_schema) in action_schemas.into_iter() {
            let deprecation = if name == NoAction.name() {
                Some("null")
            } else {
                deprecations.get(name).copied()
            };

            let include_in_unbind_target_schema =
                name != NoAction.name() && name != Unbind::name_for_type();

            // Add an alternative for plain action names.
            let mut plain_action = json_schema!({
                "type": "string",
                "const": name
            });
            if let Some(message) = deprecation_messages.get(name) {
                add_deprecation(&mut plain_action, message.to_string());
            } else if let Some(new_name) = deprecation {
                add_deprecation_preferred_name(&mut plain_action, new_name);
            }
            let description = action_documentation.get(name);
            if let Some(description) = &description {
                add_description(&mut plain_action, description);
            }
            if include_in_unbind_target_schema {
                unbind_target_action_alternatives.push(plain_action);
            }

            // Add an alternative for actions with data specified as a [name, data] array.
            //
            // When a struct with no deserializable fields is added by deriving `Action`, an empty
            // object schema is produced. The action should be invoked without data in this case.
            if let Some(schema) = action_schema
                && schema != empty_object
            {
                let mut matches_action_name = json_schema!({
                    "const": name
                });
                if let Some(description) = &description {
                    add_description(&mut matches_action_name, description);
                }
                if let Some(message) = deprecation_messages.get(name) {
                    add_deprecation(&mut matches_action_name, message.to_string());
                } else if let Some(new_name) = deprecation {
                    add_deprecation_preferred_name(&mut matches_action_name, new_name);
                }
                let action_with_input = json_schema!({
                    "type": "array",
                    "items": [matches_action_name, schema],
                    "minItems": 2,
                    "maxItems": 2
                });
                action_with_arguments_alternatives.push(action_with_input.clone());
                if include_in_unbind_target_schema {
                    unbind_target_action_alternatives.push(action_with_input);
                }
            } else {
                empty_schema_action_names.push(name);
                if include_in_unbind_target_schema {
                    empty_schema_unbind_target_action_names.push(name);
                }
            }
        }

        if !empty_schema_action_names.is_empty() {
            let action_names = json_schema!({ "enum": empty_schema_action_names });
            let no_properties_allowed = json_schema!({
                "type": "object",
                "additionalProperties": false
            });
            let mut actions_with_empty_input = json_schema!({
                "type": "array",
                "items": [action_names, no_properties_allowed],
                "minItems": 2,
                "maxItems": 2
            });
            add_deprecation(
                &mut actions_with_empty_input,
                "This action does not take input - just the action name string should be used."
                    .to_string(),
            );
            action_with_arguments_alternatives.push(actions_with_empty_input);
        }

        if !empty_schema_unbind_target_action_names.is_empty() {
            let action_names = json_schema!({ "enum": empty_schema_unbind_target_action_names });
            let no_properties_allowed = json_schema!({
                "type": "object",
                "additionalProperties": false
            });
            let mut actions_with_empty_input = json_schema!({
                "type": "array",
                "items": [action_names, no_properties_allowed],
                "minItems": 2,
                "maxItems": 2
            });
            add_deprecation(
                &mut actions_with_empty_input,
                "This action does not take input - just the action name string should be used."
                    .to_string(),
            );
            unbind_target_action_alternatives.push(actions_with_empty_input);
        }

        generator.definitions_mut().insert(
            ActionName::schema_name().to_string(),
            action_name_schema.to_value(),
        );
        generator.definitions_mut().insert(
            ActionWithArguments::schema_name().to_string(),
            json!({ "anyOf": action_with_arguments_alternatives }),
        );

        generator.definitions_mut().insert(
            KeymapAction::schema_name().to_string(),
            json!({ "anyOf": [
                { "$ref": format!("#/$defs/{}", ActionName::schema_name().to_string()) },
                { "$ref": format!("#/$defs/{}", ActionWithArguments::schema_name().to_string()) },
                { "type": "null" }
            ] }),
        );
        generator.definitions_mut().insert(
            UnbindTargetAction::schema_name().to_string(),
            json!({
                "anyOf": unbind_target_action_alternatives
            }),
        );

        generator.root_schema_for::<KeymapFile>().to_value()
    }

    pub fn sections(&self) -> impl DoubleEndedIterator<Item = &KeymapSection> {
        self.0.iter()
    }
}
