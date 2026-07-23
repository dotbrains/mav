use std::{collections::HashMap, path::Path, sync::Arc};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json_lenient::Value;
use util::command::Command;

use crate::{command_json::CommandRunner, devcontainer_api::DevContainerError};

use super::{MavCustomization, MavCustomizationsWrapper};

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct LifecycleScript {
    scripts: HashMap<String, LifecycleScriptInternal>,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
struct LifecycleScriptInternal {
    command: Option<String>,
    args: Vec<String>,
}

impl LifecycleScriptInternal {
    fn from_args(args: Vec<String>) -> Self {
        let command = args.get(0).map(|a| a.to_string());
        let remaining = args.iter().skip(1).map(|a| a.to_string()).collect();
        Self {
            command,
            args: remaining,
        }
    }
}

impl<'de> Deserialize<'de> for MavCustomizationsWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let mav = value
            .get("mav")
            .map(|mav_value| serde_json_lenient::from_value::<MavCustomization>(mav_value.clone()))
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        Ok(MavCustomizationsWrapper { mav })
    }
}

impl LifecycleScript {
    pub(crate) fn from_map(args: HashMap<String, Vec<String>>) -> Self {
        Self {
            scripts: args
                .into_iter()
                .map(|(k, v)| (k, LifecycleScriptInternal::from_args(v)))
                .collect(),
        }
    }
    pub(crate) fn from_str(args: &str) -> Self {
        let script: Vec<String> = args.split(" ").map(|a| a.to_string()).collect();

        Self::from_args(script)
    }
    pub(crate) fn from_args(args: Vec<String>) -> Self {
        Self::from_map(HashMap::from([("default".to_string(), args)]))
    }
    pub fn script_commands(&self) -> HashMap<String, Command> {
        self.scripts
            .iter()
            .filter_map(|(k, v)| {
                if let Some(inner_command) = &v.command {
                    let mut command = Command::new(inner_command);
                    command.args(&v.args);
                    Some((k.clone(), command))
                } else {
                    log::warn!(
                        "Lifecycle script command {k}, value {:?} has no program to run. Skipping",
                        v
                    );
                    None
                }
            })
            .collect()
    }

    pub async fn run(
        &self,
        command_runnder: &Arc<dyn CommandRunner>,
        working_directory: &Path,
    ) -> Result<(), DevContainerError> {
        for (command_name, mut command) in self.script_commands() {
            log::debug!("Running script {command_name}");

            command.current_dir(working_directory);

            let output = command_runnder
                .run_command(&mut command)
                .await
                .map_err(|e| {
                    log::error!("Error running command {command_name}: {e}");
                    DevContainerError::CommandFailed(command_name.clone())
                })?;
            if !output.status.success() {
                let std_err = String::from_utf8_lossy(&output.stderr);
                log::error!(
                    "Command {command_name} produced a non-successful output. StdErr: {std_err}"
                );
            }
            let std_out = String::from_utf8_lossy(&output.stdout);
            log::debug!("Command {command_name} output:\n {std_out}");
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for LifecycleScript {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct LifecycleScriptVisitor;

        impl<'de> Visitor<'de> for LifecycleScriptVisitor {
            type Value = LifecycleScript;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string, an array of strings, or a map of arrays")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(LifecycleScript::from_str(value))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut array = Vec::new();
                while let Some(elem) = seq.next_element()? {
                    array.push(elem);
                }
                Ok(LifecycleScript::from_args(array))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut result = HashMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    let value: Value = map.next_value()?;
                    let script_args = match value {
                        Value::String(s) => {
                            s.split(" ").map(|s| s.to_string()).collect::<Vec<String>>()
                        }
                        Value::Array(arr) => {
                            let strings: Vec<String> = arr
                                .into_iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                            strings
                        }
                        _ => continue,
                    };
                    result.insert(key, script_args);
                }
                Ok(LifecycleScript::from_map(result))
            }
        }

        deserializer.deserialize_any(LifecycleScriptVisitor)
    }
}
