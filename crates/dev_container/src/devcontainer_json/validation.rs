use crate::devcontainer_api::DevContainerError;

use super::{DevContainer, DevContainerBuildType};

impl DevContainer {
    pub(crate) fn build_type(&self) -> DevContainerBuildType {
        if let Some(image) = &self.image {
            DevContainerBuildType::Image(image.clone())
        } else if self.docker_compose_file.is_some() {
            DevContainerBuildType::DockerCompose
        } else if let Some(build) = &self.build {
            DevContainerBuildType::Dockerfile(build.clone())
        } else {
            DevContainerBuildType::None
        }
    }

    pub(crate) fn validate_devcontainer_contents(&self) -> Result<(), DevContainerError> {
        match self.build_type() {
            DevContainerBuildType::Image(_) => Ok(()),
            DevContainerBuildType::Dockerfile(_) => {
                if (self.workspace_folder.is_some() && self.workspace_mount.is_none())
                    || (self.workspace_folder.is_none() && self.workspace_mount.is_some())
                {
                    return Err(DevContainerError::DevContainerValidationFailed(
                        "workspaceMount and workspaceFolder must both be defined, or neither defined"
                            .to_string(),
                    ));
                }
                Ok(())
            }
            DevContainerBuildType::DockerCompose => {
                if self.service.is_none() {
                    return Err(DevContainerError::DevContainerValidationFailed(
                        "must specify a connecting service for docker-compose".to_string(),
                    ));
                }
                Ok(())
            }
            DevContainerBuildType::None => Ok(()),
        }
    }
}

// Custom deserializer that parses the entire customizations object as a
// serde_json_lenient::Value first, then extracts the "mav" portion.
// This avoids a bug in serde_json_lenient's `ignore_value` codepath which
// does not handle trailing commas in skipped values.
