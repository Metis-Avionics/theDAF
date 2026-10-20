//! Domain-owned, non-object-safe typed contracts.

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::DataAccessError;
use crate::generic::types::{MutationContext, Subject};

/// One typed read domain.
///
/// DAF supplies argument decoding and JSON boundary behavior; the implementor
/// owns the domain function and its storage/compute dependencies.
#[async_trait]
pub trait QueryDomain<P: Send + Sync + 'static>: Send + Sync {
    /// Stable resource served by this domain.
    const RESOURCE: &'static str;
    /// Domain input decoded from the boundary's JSON arguments.
    type Input: DeserializeOwned + Send;
    /// Domain output serialized to the boundary's JSON.
    type Output: Serialize + Send;

    /// Decode boundary arguments before admission-independent domain work.
    ///
    /// # Errors
    /// Defaults to [`DataAccessError::Validation`] for malformed input.
    async fn parse_arguments(arguments: &Value) -> Result<Self::Input, DataAccessError> {
        let input: Self::Input = serde_json::from_value(arguments.clone()).map_err(|error| {
            DataAccessError::Validation(crate::ValidationError::new(format!(
                "invalid arguments: {error}"
            )))
        })?;
        Ok(input)
    }

    /// Execute the domain read.
    ///
    /// # Errors
    /// Domain-owned failures are mapped to DAF's structured taxonomy.
    async fn run(
        &self,
        pool: &P,
        input: Self::Input,
        subject: &Subject,
    ) -> Result<Self::Output, DataAccessError>;
}

/// One typed write domain.
///
/// The optional [`MutationDomain::idempotency_key`] returns a stable key only
/// for retryable writes. Keys must include the mutating subject so one
/// caller's replay cannot return another caller's result.
#[async_trait]
pub trait MutationDomain<P: Send + Sync + 'static>: Send + Sync {
    /// Stable resource served by this domain.
    const RESOURCE: &'static str;
    /// Domain input decoded from the boundary's JSON arguments.
    type Input: DeserializeOwned + Send;
    /// Domain mutation result serialized to the boundary's JSON.
    type Output: Serialize + Send;

    /// Decode boundary arguments before authorization-dependent domain work.
    ///
    /// # Errors
    /// Defaults to [`DataAccessError::Validation`] for malformed input.
    async fn parse_arguments(arguments: &Value) -> Result<Self::Input, DataAccessError> {
        let input: Self::Input = serde_json::from_value(arguments.clone()).map_err(|error| {
            DataAccessError::Validation(crate::ValidationError::new(format!(
                "invalid arguments: {error}"
            )))
        })?;
        Ok(input)
    }

    /// Derive an idempotency key, or `None` when the write is not replayable.
    #[must_use]
    fn idempotency_key(&self, _subject: &Subject, _input: &Self::Input) -> Option<String> {
        debug_assert!(!Self::RESOURCE.is_empty(), "replays name their resource");
        None
    }

    /// Execute the domain write.
    ///
    /// # Errors
    /// Domain-owned failures are mapped to DAF's structured taxonomy.
    async fn run(
        &self,
        pool: &P,
        input: Self::Input,
        subject: &Subject,
        context: &MutationContext,
    ) -> Result<Self::Output, DataAccessError>;
}
