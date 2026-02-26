//! Retry wrapper for LLM providers.
//!
//! `RetryProvider` wraps any `LlmProvider` and retries transient failures
//! (network errors, HTTP 5xx, rate limits) with exponential backoff and jitter.

use async_trait::async_trait;
use futures::stream::BoxStream;
use gagent_core::GagentError;
use std::time::Duration;
use tracing::{info, warn};

use crate::message::StreamChunk;
use crate::provider::{ChatRequest, ChatResponse, LlmProvider};

/// Configuration for the retry policy.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (1 means no retries).
    pub max_attempts: u32,

    /// Initial delay before the first retry.
    pub initial_delay: Duration,

    /// Multiplier applied to the delay after each attempt.
    pub backoff_factor: f64,

    /// Maximum delay cap.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Wraps any `LlmProvider` with automatic retry logic.
pub struct RetryProvider<P: LlmProvider> {
    inner: P,
    config: RetryConfig,
}

impl<P: LlmProvider> RetryProvider<P> {
    pub fn new(inner: P, config: RetryConfig) -> Self {
        Self { inner, config }
    }

    pub fn with_defaults(inner: P) -> Self {
        Self::new(inner, RetryConfig::default())
    }
}

/// Determine whether an error is retryable.
fn is_retryable(error: &GagentError) -> bool {
    match error {
        // Network / HTTP errors are generally transient
        GagentError::Http(_) => true,

        // LLM errors may include rate limits (5xx / 429)
        GagentError::Llm(msg) => {
            msg.contains("429")
                || msg.contains("500")
                || msg.contains("502")
                || msg.contains("503")
                || msg.contains("504")
                || msg.contains("rate limit")
                || msg.contains("overloaded")
        }

        // Timeouts are retryable
        GagentError::Timeout(_) => true,

        // Everything else is not retryable
        _ => false,
    }
}

/// Compute the delay for a given attempt using exponential backoff.
fn backoff_delay(attempt: u32, config: &RetryConfig) -> Duration {
    let base_ms = config.initial_delay.as_millis() as f64;
    let factor = config.backoff_factor.powi(attempt as i32);
    let delay_ms = (base_ms * factor).min(config.max_delay.as_millis() as f64);
    Duration::from_millis(delay_ms as u64)
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for RetryProvider<P> {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, GagentError> {
        let mut last_err = GagentError::Other("No attempts made".to_string());

        for attempt in 0..self.config.max_attempts {
            match self.inner.chat(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) if attempt + 1 < self.config.max_attempts && is_retryable(&e) => {
                    let delay = backoff_delay(attempt, &self.config);
                    warn!(
                        "LLM request failed (attempt {}/{}): {}. Retrying in {:?}...",
                        attempt + 1,
                        self.config.max_attempts,
                        e,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                    last_err = e;
                }
                Err(e) => {
                    info!(
                        "LLM request failed (attempt {}/{}): {}. Not retrying.",
                        attempt + 1,
                        self.config.max_attempts,
                        e
                    );
                    return Err(e);
                }
            }
        }

        Err(last_err)
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, GagentError>>, GagentError> {
        // Streaming is not retried because the stream may have partially consumed;
        // just delegate directly to the inner provider.
        self.inner.chat_stream(request).await
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_increases() {
        let config = RetryConfig {
            initial_delay: Duration::from_millis(100),
            backoff_factor: 2.0,
            max_delay: Duration::from_secs(60),
            max_attempts: 5,
        };

        let delay0 = backoff_delay(0, &config);
        let delay1 = backoff_delay(1, &config);
        let delay2 = backoff_delay(2, &config);

        assert_eq!(delay0, Duration::from_millis(100));
        assert_eq!(delay1, Duration::from_millis(200));
        assert_eq!(delay2, Duration::from_millis(400));
    }

    #[test]
    fn test_backoff_delay_capped_at_max() {
        let config = RetryConfig {
            initial_delay: Duration::from_millis(1000),
            backoff_factor: 10.0,
            max_delay: Duration::from_secs(5),
            max_attempts: 5,
        };

        let delay = backoff_delay(5, &config);
        assert!(delay <= Duration::from_secs(5));
    }

    #[test]
    fn test_is_retryable_http_error() {
        assert!(is_retryable(&GagentError::Http("connection refused".to_string())));
    }

    #[test]
    fn test_is_retryable_rate_limit() {
        assert!(is_retryable(&GagentError::Llm("429 rate limit exceeded".to_string())));
    }

    #[test]
    fn test_is_retryable_5xx() {
        assert!(is_retryable(&GagentError::Llm("503 Service Unavailable".to_string())));
    }

    #[test]
    fn test_not_retryable_config_error() {
        assert!(!is_retryable(&GagentError::Config("bad config".to_string())));
    }

    #[test]
    fn test_not_retryable_path_error() {
        assert!(!is_retryable(&GagentError::PathNotAllowed("/etc/passwd".to_string())));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff_factor, 2.0);
    }
}
