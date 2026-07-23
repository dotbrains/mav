use super::*;

impl Model {
    pub fn default_fast() -> Self {
        Self::ClaudeHaiku4_5
    }

    pub fn default_go() -> Self {
        Self::KimiK2_6
    }

    pub fn default_go_fast() -> Self {
        Self::MiniMaxM2_7
    }

    pub fn default_free() -> Self {
        Self::BigPickle
    }

    pub fn default_free_fast() -> Self {
        Self::Nemotron3UltraFree
    }

    pub fn available_subscriptions(&self) -> &'static [OpenCodeSubscription] {
        match self {
            // Models available in both Zen and Go
            Self::Glm5_1
            | Self::KimiK2_6
            | Self::MiniMaxM2_7
            | Self::DeepSeekV4Pro
            | Self::DeepSeekV4Flash
            | Self::Qwen3_6Plus => &[OpenCodeSubscription::Zen, OpenCodeSubscription::Go],

            // Go-only models
            Self::MimoV2_5Pro
            | Self::MimoV2_5
            | Self::Qwen3_7Plus
            | Self::Qwen3_7Max
            | Self::KimiK2_7Code
            | Self::Glm5_2
            | Self::MiniMaxM3 => &[OpenCodeSubscription::Go],

            // Deprecated on Go (per models.dev); still offered on Zen
            Self::Glm5 | Self::KimiK2_5 | Self::MiniMaxM2_5 => &[OpenCodeSubscription::Zen],

            // Free models
            Self::Nemotron3UltraFree | Self::BigPickle => &[OpenCodeSubscription::Free],

            // Custom models get their subscription from settings, not from here
            Self::Custom { .. } => &[],

            // All other built-in models are Zen-only
            _ => &[OpenCodeSubscription::Zen],
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::ClaudeOpus4_8 => "claude-opus-4-8",
            Self::ClaudeOpus4_7 => "claude-opus-4-7",
            Self::ClaudeOpus4_6 => "claude-opus-4-6",
            Self::ClaudeOpus4_5 => "claude-opus-4-5",
            Self::ClaudeOpus4_1 => "claude-opus-4-1",
            Self::ClaudeSonnet4_6 => "claude-sonnet-4-6",
            Self::ClaudeSonnet4_5 => "claude-sonnet-4-5",
            Self::ClaudeSonnet4 => "claude-sonnet-4",
            Self::ClaudeHaiku4_5 => "claude-haiku-4-5",

            Self::Gpt5_5 => "gpt-5.5",
            Self::Gpt5_5Pro => "gpt-5.5-pro",
            Self::Gpt5_4 => "gpt-5.4",
            Self::Gpt5_4Pro => "gpt-5.4-pro",
            Self::Gpt5_4Mini => "gpt-5.4-mini",
            Self::Gpt5_4Nano => "gpt-5.4-nano",
            Self::Gpt5_3Codex => "gpt-5.3-codex",
            Self::Gpt5_3Spark => "gpt-5.3-codex-spark",
            Self::Gpt5_2 => "gpt-5.2",
            Self::Gpt5_2Codex => "gpt-5.2-codex",
            Self::Gpt5_1 => "gpt-5.1",
            Self::Gpt5_1Codex => "gpt-5.1-codex",
            Self::Gpt5_1CodexMax => "gpt-5.1-codex-max",
            Self::Gpt5_1CodexMini => "gpt-5.1-codex-mini",
            Self::Gpt5 => "gpt-5",
            Self::Gpt5Codex => "gpt-5-codex",
            Self::Gpt5Nano => "gpt-5-nano",

            Self::Gemini3_1Pro => "gemini-3.1-pro",
            Self::Gemini3Flash => "gemini-3-flash",
            Self::Gemini3_5Flash => "gemini-3.5-flash",

            Self::DeepSeekV4Pro => "deepseek-v4-pro",
            Self::DeepSeekV4Flash => "deepseek-v4-flash",
            Self::MiniMaxM2_5 => "minimax-m2.5",
            Self::Glm5 => "glm-5",
            Self::Glm5_1 => "glm-5.1",
            Self::Glm5_2 => "glm-5.2",
            Self::GrokBuild0_1 => "grok-build-0.1",
            Self::KimiK2_5 => "kimi-k2.5",
            Self::KimiK2_6 => "kimi-k2.6",
            Self::KimiK2_7Code => "kimi-k2.7-code",
            Self::MiniMaxM2_7 => "minimax-m2.7",
            Self::MiniMaxM3 => "minimax-m3",
            Self::MimoV2_5Pro => "mimo-v2.5-pro",
            Self::MimoV2_5 => "mimo-v2.5",
            Self::Qwen3_5Plus => "qwen3.5-plus",
            Self::Qwen3_6Plus => "qwen3.6-plus",
            Self::Qwen3_7Plus => "qwen3.7-plus",
            Self::Qwen3_7Max => "qwen3.7-max",
            Self::BigPickle => "big-pickle",
            Self::Nemotron3UltraFree => "nemotron-3-ultra-free",

            Self::Custom { name, .. } => name,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::ClaudeOpus4_8 => "Claude Opus 4.8",
            Self::ClaudeOpus4_7 => "Claude Opus 4.7",
            Self::ClaudeOpus4_6 => "Claude Opus 4.6",
            Self::ClaudeOpus4_5 => "Claude Opus 4.5",
            Self::ClaudeOpus4_1 => "Claude Opus 4.1",
            Self::ClaudeSonnet4_6 => "Claude Sonnet 4.6",
            Self::ClaudeSonnet4_5 => "Claude Sonnet 4.5",
            Self::ClaudeSonnet4 => "Claude Sonnet 4",
            Self::ClaudeHaiku4_5 => "Claude Haiku 4.5",

            Self::Gpt5_5 => "GPT 5.5",
            Self::Gpt5_5Pro => "GPT 5.5 Pro",
            Self::Gpt5_4 => "GPT 5.4",
            Self::Gpt5_4Pro => "GPT 5.4 Pro",
            Self::Gpt5_4Mini => "GPT 5.4 Mini",
            Self::Gpt5_4Nano => "GPT 5.4 Nano",
            Self::Gpt5_3Codex => "GPT 5.3 Codex",
            Self::Gpt5_3Spark => "GPT 5.3 Codex Spark",
            Self::Gpt5_2 => "GPT 5.2",
            Self::Gpt5_2Codex => "GPT 5.2 Codex",
            Self::Gpt5_1 => "GPT 5.1",
            Self::Gpt5_1Codex => "GPT 5.1 Codex",
            Self::Gpt5_1CodexMax => "GPT 5.1 Codex Max",
            Self::Gpt5_1CodexMini => "GPT 5.1 Codex Mini",
            Self::Gpt5 => "GPT 5",
            Self::Gpt5Codex => "GPT 5 Codex",
            Self::Gpt5Nano => "GPT 5 Nano",

            Self::Gemini3_1Pro => "Gemini 3.1 Pro",
            Self::Gemini3Flash => "Gemini 3 Flash",
            Self::Gemini3_5Flash => "Gemini 3.5 Flash",

            Self::DeepSeekV4Pro => "DeepSeek V4 Pro",
            Self::DeepSeekV4Flash => "DeepSeek V4 Flash",
            Self::MiniMaxM2_5 => "MiniMax M2.5",
            Self::Glm5 => "GLM 5",
            Self::Glm5_1 => "GLM 5.1",
            Self::Glm5_2 => "GLM 5.2",
            Self::GrokBuild0_1 => "Grok Build 0.1",
            Self::KimiK2_5 => "Kimi K2.5",
            Self::KimiK2_6 => "Kimi K2.6",
            Self::KimiK2_7Code => "Kimi K2.7 Code",
            Self::MiniMaxM2_7 => "MiniMax M2.7",
            Self::MiniMaxM3 => "MiniMax M3",
            Self::MimoV2_5Pro => "MiMo V2.5 Pro",
            Self::MimoV2_5 => "MiMo V2.5",
            Self::Qwen3_5Plus => "Qwen3.5 Plus",
            Self::Qwen3_6Plus => "Qwen3.6 Plus",
            Self::Qwen3_7Plus => "Qwen3.7 Plus",
            Self::Qwen3_7Max => "Qwen3.7 Max",
            Self::BigPickle => "Big Pickle",
            Self::Nemotron3UltraFree => "Nemotron 3 Ultra Free",

            Self::Custom {
                name, display_name, ..
            } => display_name.as_deref().unwrap_or(name),
        }
    }

    pub fn protocol(&self, subscription: OpenCodeSubscription) -> ApiProtocol {
        match self {
            // Models offered by OpenCode have the same configuration across subscriptions
            //  with one outlier: non-free MiniMax models
            Self::MiniMaxM2_7 | Self::MiniMaxM2_5 => {
                if subscription == OpenCodeSubscription::Zen {
                    ApiProtocol::OpenAiChat
                } else {
                    ApiProtocol::Anthropic
                }
            }

            Self::ClaudeOpus4_8
            | Self::ClaudeOpus4_7
            | Self::ClaudeOpus4_6
            | Self::ClaudeOpus4_5
            | Self::ClaudeOpus4_1
            | Self::ClaudeSonnet4_6
            | Self::ClaudeSonnet4_5
            | Self::ClaudeSonnet4
            | Self::ClaudeHaiku4_5 => ApiProtocol::Anthropic,

            Self::Gpt5_5
            | Self::Gpt5_5Pro
            | Self::Gpt5_4
            | Self::Gpt5_4Pro
            | Self::Gpt5_4Mini
            | Self::Gpt5_4Nano
            | Self::Gpt5_3Codex
            | Self::Gpt5_3Spark
            | Self::Gpt5_2
            | Self::Gpt5_2Codex
            | Self::Gpt5_1
            | Self::Gpt5_1Codex
            | Self::Gpt5_1CodexMax
            | Self::Gpt5_1CodexMini
            | Self::Gpt5
            | Self::Gpt5Codex
            | Self::Gpt5Nano => ApiProtocol::OpenAiResponses,

            Self::Gemini3_1Pro | Self::Gemini3Flash | Self::Gemini3_5Flash => ApiProtocol::Google,

            Self::Qwen3_7Max | Self::Qwen3_7Plus => ApiProtocol::Anthropic,

            Self::MiniMaxM3 => ApiProtocol::Anthropic,

            Self::Glm5
            | Self::Glm5_1
            | Self::Glm5_2
            | Self::GrokBuild0_1
            | Self::KimiK2_5
            | Self::KimiK2_6
            | Self::KimiK2_7Code
            | Self::MimoV2_5Pro
            | Self::MimoV2_5
            | Self::Qwen3_5Plus
            | Self::Qwen3_6Plus
            | Self::DeepSeekV4Pro
            | Self::DeepSeekV4Flash
            | Self::BigPickle
            | Self::Nemotron3UltraFree => ApiProtocol::OpenAiChat,

            Self::Custom { protocol, .. } => *protocol,
        }
    }
}
