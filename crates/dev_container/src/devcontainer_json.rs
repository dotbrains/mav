mod deserializers;
mod lifecycle;
mod types;
mod validation;

pub(crate) use deserializers::{
    deserialize_devcontainer_json, deserialize_devcontainer_json_from_value,
    deserialize_devcontainer_json_to_value,
};
pub(crate) use lifecycle::LifecycleScript;
pub(crate) use types::*;

#[cfg(test)]
mod tests;
