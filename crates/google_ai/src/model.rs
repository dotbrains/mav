use super::{GoogleModelMode, ThinkingLevel};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq, strum::EnumIter)]
pub enum Model {
    #[serde(
        rename = "gemini-2.5-flash-lite",
        alias = "gemini-2.5-flash-lite-preview-06-17",
        alias = "gemini-2.0-flash-lite-preview"
    )]
    Gemini25FlashLite,
    #[serde(
        rename = "gemini-2.5-flash",
        alias = "gemini-2.0-flash-thinking-exp",
        alias = "gemini-2.5-flash-preview-04-17",
        alias = "gemini-2.5-flash-preview-05-20",
        alias = "gemini-2.5-flash-preview-latest",
        alias = "gemini-2.0-flash"
    )]
    #[default]
    Gemini25Flash,
    #[serde(
        rename = "gemini-2.5-pro",
        alias = "gemini-2.0-pro-exp",
        alias = "gemini-2.5-pro-preview-latest",
        alias = "gemini-2.5-pro-exp-03-25",
        alias = "gemini-2.5-pro-preview-03-25",
        alias = "gemini-2.5-pro-preview-05-06",
        alias = "gemini-2.5-pro-preview-06-05"
    )]
    Gemini25Pro,
    #[serde(rename = "gemini-3.1-flash-lite")]
    Gemini31FlashLite,
    #[serde(rename = "gemini-3-flash-preview")]
    Gemini3Flash,
    #[serde(rename = "gemini-3.5-flash")]
    Gemini35Flash,
    #[serde(rename = "gemini-3.1-pro-preview", alias = "gemini-3-pro-preview")]
    Gemini31Pro,
    #[serde(rename = "custom")]
    Custom {
        name: String,
        /// The name displayed in the UI, such as in the agent panel model dropdown menu.
        display_name: Option<String>,
        max_tokens: u64,
        #[serde(default)]
        mode: GoogleModelMode,
    },
}

impl Model {
    pub fn default_fast() -> Self {
        Self::Gemini31FlashLite
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "gemini-2.5-flash-lite",
            Self::Gemini25Flash => "gemini-2.5-flash",
            Self::Gemini25Pro => "gemini-2.5-pro",
            Self::Gemini31FlashLite => "gemini-3.1-flash-lite",
            Self::Gemini3Flash => "gemini-3-flash-preview",
            Self::Gemini35Flash => "gemini-3.5-flash",
            Self::Gemini31Pro => "gemini-3.1-pro-preview",
            Self::Custom { name, .. } => name,
        }
    }
    pub fn request_id(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "gemini-2.5-flash-lite",
            Self::Gemini25Flash => "gemini-2.5-flash",
            Self::Gemini25Pro => "gemini-2.5-pro",
            Self::Gemini31FlashLite => "gemini-3.1-flash-lite",
            Self::Gemini3Flash => "gemini-3-flash-preview",
            Self::Gemini35Flash => "gemini-3.5-flash",
            Self::Gemini31Pro => "gemini-3.1-pro-preview",
            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Gemini25FlashLite => "Gemini 2.5 Flash-Lite",
            Self::Gemini25Flash => "Gemini 2.5 Flash",
            Self::Gemini25Pro => "Gemini 2.5 Pro",
            Self::Gemini31FlashLite => "Gemini 3.1 Flash-Lite",
            Self::Gemini3Flash => "Gemini 3 Flash",
            Self::Gemini35Flash => "Gemini 3.5 Flash",
            Self::Gemini31Pro => "Gemini 3.1 Pro",
            Self::Custom {
                name, display_name, ..
            } => display_name.as_ref().unwrap_or(name),
        }
    }

    pub fn max_token_count(&self) -> u64 {
        match self {
            Self::Gemini25FlashLite
            | Self::Gemini25Flash
            | Self::Gemini25Pro
            | Self::Gemini31FlashLite
            | Self::Gemini3Flash
            | Self::Gemini35Flash
            | Self::Gemini31Pro => 1_048_576,
            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self) -> Option<u64> {
        match self {
            Model::Gemini25FlashLite
            | Model::Gemini25Flash
            | Model::Gemini25Pro
            | Model::Gemini31FlashLite
            | Model::Gemini3Flash
            | Model::Gemini35Flash
            | Model::Gemini31Pro => Some(65_536),
            Model::Custom { .. } => None,
        }
    }

    pub fn supports_tools(&self) -> bool {
        true
    }

    pub fn supports_images(&self) -> bool {
        true
    }

    pub fn supports_thinking(&self) -> bool {
        matches!(
            self,
            Self::Gemini25FlashLite
                | Self::Gemini25Flash
                | Self::Gemini25Pro
                | Self::Gemini31FlashLite
                | Self::Gemini3Flash
                | Self::Gemini35Flash
                | Self::Gemini31Pro
                | Self::Custom {
                    mode: GoogleModelMode::Thinking { .. },
                    ..
                }
        )
    }

    pub fn supported_thinking_levels(&self) -> &'static [ThinkingLevel] {
        match self {
            Self::Gemini31FlashLite | Self::Gemini3Flash | Self::Gemini35Flash => &[
                ThinkingLevel::Minimal,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            Self::Gemini31Pro => &[
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
            ],
            _ => &[],
        }
    }

    pub fn default_thinking_level(&self) -> Option<ThinkingLevel> {
        match self {
            Self::Gemini31FlashLite => Some(ThinkingLevel::Minimal),
            Self::Gemini3Flash => Some(ThinkingLevel::High),
            Self::Gemini35Flash => Some(ThinkingLevel::Medium),
            Self::Gemini31Pro => Some(ThinkingLevel::High),
            _ => None,
        }
    }

    pub fn mode(&self) -> GoogleModelMode {
        match self {
            Self::Gemini25FlashLite | Self::Gemini25Flash | Self::Gemini25Pro => {
                GoogleModelMode::Thinking {
                    // By default these models are set to "auto", so we preserve that behavior
                    // but indicate they are capable of thinking mode
                    budget_tokens: None,
                }
            }
            Self::Gemini31FlashLite
            | Self::Gemini3Flash
            | Self::Gemini35Flash
            | Self::Gemini31Pro => GoogleModelMode::Thinking {
                budget_tokens: None,
            },
            Self::Custom { mode, .. } => *mode,
        }
    }
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}
