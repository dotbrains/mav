use super::*;

impl Model {
    pub fn interleaved_reasoning(&self) -> bool {
        match self {
            Self::DeepSeekV4Pro
            | Self::DeepSeekV4Flash
            | Self::KimiK2_5
            | Self::KimiK2_6
            | Self::KimiK2_7Code
            | Self::MimoV2_5
            | Self::MimoV2_5Pro
            | Self::Glm5
            | Self::Glm5_1
            | Self::Glm5_2
            | Self::MiniMaxM2_5
            | Self::MiniMaxM2_7
            | Self::Nemotron3UltraFree
            | Self::BigPickle => true,

            Self::Custom {
                interleaved_reasoning,
                ..
            } => *interleaved_reasoning,

            _ => false,
        }
    }

    pub fn max_token_count(&self, subscription: OpenCodeSubscription) -> u64 {
        match self {
            // Anthropic models
            Self::ClaudeOpus4_8 | Self::ClaudeOpus4_7 => 1_000_000,
            Self::ClaudeOpus4_6 | Self::ClaudeSonnet4_6 => 1_000_000,
            Self::ClaudeSonnet4_5 => 1_000_000,
            Self::ClaudeOpus4_5 | Self::ClaudeHaiku4_5 => 200_000,
            Self::ClaudeOpus4_1 => 200_000,
            Self::ClaudeSonnet4 => 1_000_000,

            // OpenAI models
            Self::Gpt5_5 | Self::Gpt5_5Pro => 1_050_000,
            Self::Gpt5_4 | Self::Gpt5_4Pro => 1_050_000,
            Self::Gpt5_4Mini | Self::Gpt5_4Nano => 400_000,
            Self::Gpt5_3Codex => 400_000,
            Self::Gpt5_3Spark => 128_000,
            Self::Gpt5_2 | Self::Gpt5_2Codex => 400_000,
            Self::Gpt5_1 | Self::Gpt5_1Codex | Self::Gpt5_1CodexMax | Self::Gpt5_1CodexMini => {
                400_000
            }
            Self::Gpt5 | Self::Gpt5Codex | Self::Gpt5Nano => 400_000,

            // Google models
            Self::Gemini3_1Pro => 1_048_576,
            Self::Gemini3Flash => 1_048_576,
            Self::Gemini3_5Flash => 1_048_576,

            // OpenAI-compatible models
            Self::MiniMaxM2_7 => 204_800,
            Self::MiniMaxM3 => 512_000,
            Self::MiniMaxM2_5 => 204_800,
            Self::Glm5 | Self::Glm5_1 => {
                if subscription == OpenCodeSubscription::Go {
                    202_752
                } else {
                    204_800
                }
            }
            Self::Glm5_2 => 1_000_000,
            Self::KimiK2_6 | Self::KimiK2_5 | Self::KimiK2_7Code => 262_144,
            Self::GrokBuild0_1 => 256_000,
            Self::MimoV2_5Pro => 1_048_576,
            Self::MimoV2_5 => 1_000_000,
            Self::Qwen3_5Plus => 262_144,
            Self::Qwen3_6Plus => {
                if subscription == OpenCodeSubscription::Go {
                    1_000_000
                } else {
                    262_144
                }
            }
            Self::Qwen3_7Max | Self::Qwen3_7Plus => 1_000_000,
            Self::BigPickle => 200_000,
            Self::Nemotron3UltraFree => 1_000_000,
            Self::DeepSeekV4Pro | Self::DeepSeekV4Flash => 1_000_000,

            Self::Custom { max_tokens, .. } => *max_tokens,
        }
    }

    pub fn max_output_tokens(&self, subscription: OpenCodeSubscription) -> Option<u64> {
        match self {
            // Anthropic models
            Self::ClaudeOpus4_8 | Self::ClaudeOpus4_7 | Self::ClaudeOpus4_6 => Some(128_000),
            Self::ClaudeOpus4_5
            | Self::ClaudeSonnet4_6
            | Self::ClaudeSonnet4_5
            | Self::ClaudeHaiku4_5
            | Self::ClaudeSonnet4 => Some(64_000),
            Self::ClaudeOpus4_1 => Some(32_000),

            // OpenAI models
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
            | Self::Gpt5Nano => Some(128_000),

            // Google models
            Self::Gemini3_1Pro | Self::Gemini3Flash | Self::Gemini3_5Flash => Some(65_536),

            // OpenAI-compatible models
            Self::MiniMaxM2_7 => Some(131_072),
            Self::MiniMaxM3 => Some(131_072),
            Self::MiniMaxM2_5 => {
                if subscription == OpenCodeSubscription::Go {
                    Some(65_536)
                } else {
                    Some(131_072)
                }
            }
            Self::Glm5 | Self::Glm5_1 => {
                if subscription == OpenCodeSubscription::Go {
                    Some(32_768)
                } else {
                    Some(131_072)
                }
            }
            Self::Glm5_2 => Some(131_072),
            Self::BigPickle => Some(32_000),
            Self::KimiK2_6 | Self::KimiK2_5 => Some(65_536),
            Self::KimiK2_7Code => Some(262_144),
            Self::GrokBuild0_1 => Some(256_000),
            Self::Qwen3_7Max | Self::Qwen3_7Plus | Self::Qwen3_6Plus | Self::Qwen3_5Plus => {
                Some(65_536)
            }
            Self::DeepSeekV4Pro | Self::DeepSeekV4Flash => Some(384_000),
            Self::Nemotron3UltraFree => Some(128_000),
            Self::MimoV2_5Pro | Self::MimoV2_5 => Some(128_000),

            Self::Custom {
                max_output_tokens, ..
            } => *max_output_tokens,
        }
    }

    pub fn supports_tools(&self) -> bool {
        true
    }

    pub fn supports_images(&self) -> bool {
        match self {
            // Anthropic models support images
            Self::ClaudeOpus4_8
            | Self::ClaudeOpus4_7
            | Self::ClaudeOpus4_6
            | Self::ClaudeOpus4_5
            | Self::ClaudeOpus4_1
            | Self::ClaudeSonnet4_6
            | Self::ClaudeSonnet4_5
            | Self::ClaudeSonnet4
            | Self::ClaudeHaiku4_5 => true,

            // OpenAI models support images
            Self::Gpt5_5
            | Self::Gpt5_5Pro
            | Self::Gpt5_4
            | Self::Gpt5_4Pro
            | Self::Gpt5_4Mini
            | Self::Gpt5_4Nano
            | Self::Gpt5_3Codex
            | Self::Gpt5_2
            | Self::Gpt5_2Codex
            | Self::Gpt5_1
            | Self::Gpt5_1Codex
            | Self::Gpt5_1CodexMax
            | Self::Gpt5_1CodexMini
            | Self::Gpt5
            | Self::Gpt5Codex
            | Self::Gpt5Nano => true,

            // OpenAI models without image support
            Self::Gpt5_3Spark => false,

            // Google models support images
            Self::Gemini3_1Pro | Self::Gemini3Flash | Self::Gemini3_5Flash => true,

            // OpenAI-compatible models with image support
            Self::KimiK2_6
            | Self::KimiK2_7Code
            | Self::KimiK2_5
            | Self::GrokBuild0_1
            | Self::MimoV2_5
            | Self::Qwen3_5Plus
            | Self::Qwen3_6Plus
            | Self::Qwen3_7Plus
            | Self::MiniMaxM3 => true,

            // OpenAI-compatible models without image support
            Self::MiniMaxM2_5
            | Self::Glm5
            | Self::Glm5_1
            | Self::Glm5_2
            | Self::MiniMaxM2_7
            | Self::MimoV2_5Pro
            | Self::DeepSeekV4Pro
            | Self::DeepSeekV4Flash
            | Self::Qwen3_7Max
            | Self::BigPickle
            | Self::Nemotron3UltraFree => false,

            Self::Custom { protocol, .. } => matches!(
                protocol,
                ApiProtocol::Anthropic
                    | ApiProtocol::OpenAiResponses
                    | ApiProtocol::OpenAiChat
                    | ApiProtocol::Google
            ),
        }
    }

    pub fn supported_reasoning_effort_levels(&self) -> Option<Vec<ReasoningEffort>> {
        match self {
            Self::ClaudeOpus4_8 => Some(vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
            ]),

            Self::Nemotron3UltraFree | Self::MimoV2_5Pro | Self::MimoV2_5 => Some(vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ]),

            Self::DeepSeekV4Pro | Self::DeepSeekV4Flash => Some(vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::Max,
            ]),

            Self::Custom {
                reasoning_effort_levels,
                ..
            } => reasoning_effort_levels.clone(),

            _ => None,
        }
    }
}
