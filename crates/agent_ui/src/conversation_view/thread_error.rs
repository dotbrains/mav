use super::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum ThreadFeedback {
    Positive,
    Negative,
}

#[derive(Debug)]
pub(crate) enum ThreadError {
    PaymentRequired,
    DataRetentionConsentRequired,
    Refusal,
    AuthenticationRequired(SharedString),
    RateLimitExceeded {
        provider: SharedString,
    },
    ServerOverloaded {
        provider: SharedString,
    },
    PromptTooLarge,
    NoCredentials {
        provider: SharedString,
    },
    StreamError {
        provider: SharedString,
    },
    AuthenticationFailed {
        provider: SharedString,
    },
    PermissionDenied {
        provider: SharedString,
        message: Option<SharedString>,
    },
    RequestFailed,
    MaxOutputTokens,
    NoModelSelected,
    ApiError {
        provider: SharedString,
    },
    Other {
        message: SharedString,
        acp_error_code: Option<SharedString>,
    },
}

impl From<anyhow::Error> for ThreadError {
    fn from(error: anyhow::Error) -> Self {
        if error.is::<MaxOutputTokensError>() {
            Self::MaxOutputTokens
        } else if error.is::<NoModelConfiguredError>() {
            Self::NoModelSelected
        } else if let Some(acp_error) = error.downcast_ref::<acp::Error>()
            && acp_error.code == acp::ErrorCode::AuthRequired
        {
            Self::AuthenticationRequired(acp_error.message.clone().into())
        } else if let Some(lm_error) = error.downcast_ref::<LanguageModelCompletionError>() {
            use LanguageModelCompletionError::*;
            match lm_error {
                RateLimitExceeded { provider, .. } => Self::RateLimitExceeded {
                    provider: provider.to_string().into(),
                },
                ServerOverloaded { provider, .. } | ApiInternalServerError { provider, .. } => {
                    Self::ServerOverloaded {
                        provider: provider.to_string().into(),
                    }
                }
                PromptTooLarge { .. } => Self::PromptTooLarge,
                PaymentRequired => Self::PaymentRequired,
                NoApiKey { provider } => Self::NoCredentials {
                    provider: provider.to_string().into(),
                },
                StreamEndedUnexpectedly { provider }
                | ApiReadResponseError { provider, .. }
                | DeserializeResponse { provider, .. }
                | HttpSend { provider, .. } => Self::StreamError {
                    provider: provider.to_string().into(),
                },
                AuthenticationError { provider, .. } => Self::AuthenticationFailed {
                    provider: provider.to_string().into(),
                },
                PermissionError { provider, message } => Self::PermissionDenied {
                    provider: provider.to_string().into(),
                    message: Some(message.clone().into()),
                },
                UpstreamProviderError { .. } => Self::RequestFailed,
                DataRetentionConsentRequired { .. } => Self::DataRetentionConsentRequired,
                BadRequestFormat { provider, .. }
                | HttpResponseError { provider, .. }
                | ApiEndpointNotFound { provider } => Self::ApiError {
                    provider: provider.to_string().into(),
                },
                _ => {
                    let message: SharedString = format!("{:#}", error).into();
                    Self::Other {
                        message,
                        acp_error_code: None,
                    }
                }
            }
        } else {
            let message: SharedString = format!("{:#}", error).into();

            // Extract ACP error code if available
            let acp_error_code = error
                .downcast_ref::<acp::Error>()
                .map(|acp_error| SharedString::from(acp_error.code.to_string()));

            Self::Other {
                message,
                acp_error_code,
            }
        }
    }
}
