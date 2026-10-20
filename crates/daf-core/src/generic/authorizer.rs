//! Static-policy authorization over typed domain resources.

use crate::DataAccessError;
use crate::generic::policy::{AdmissionMode, ResourcePolicy};
use crate::generic::types::Subject;

/// A policy-owned authority with query and mutation tables.
pub trait PolicyTable: Send + Sync {
    /// The fail-closed table.
    fn policies() -> &'static [ResourcePolicy]
    where
        Self: Sized;
}

/// Object-safe authorization adapter over a static [`PolicyTable`].
#[derive(Debug, Clone, Copy, Default)]
pub struct StaticPolicyAuthorizer<T> {
    table: std::marker::PhantomData<T>,
}

impl<T: PolicyTable> StaticPolicyAuthorizer<T> {
    /// Build an authority around a static table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            table: std::marker::PhantomData,
        }
    }

    fn policy(&self, resource: &str) -> Option<(Option<AdmissionMode>, Option<AdmissionMode>)> {
        debug_assert!(!resource.is_empty(), "policy resources are named");
        let mut modes = (None, None);
        for policy in <T as PolicyTable>::policies() {
            if policy.resource == resource {
                modes.0 = modes.0.or(policy.query);
                modes.1 = modes.1.or(policy.mutation);
            }
        }
        (modes.0.is_some() || modes.1.is_some()).then_some((modes.0, modes.1))
    }

    fn refusal(&self, resource: &str, operation: &str) -> DataAccessError {
        debug_assert!(!operation.is_empty(), "policy refusals name the operation");
        DataAccessError::Authorization(crate::AuthorizationError::new(format!(
            "'{resource}' is not a served {operation} resource"
        )))
    }

    /// Check a read.
    ///
    /// # Errors
    /// Authorization when absent or not read-admitted.
    pub fn authorize_query(
        &self,
        subject: &Subject,
        resource: &str,
    ) -> Result<(), DataAccessError> {
        debug_assert!(!resource.is_empty(), "query policies name the resource");
        self.policy(resource)
            .and_then(|(query, _)| query)
            .ok_or_else(|| self.refusal(resource, "query"))?
            .authorize(subject, resource)
    }

    /// Check a write.
    ///
    /// # Errors
    /// Authorization when absent or not mutation-admitted.
    pub fn authorize_mutation(
        &self,
        subject: &Subject,
        resource: &str,
    ) -> Result<(), DataAccessError> {
        debug_assert!(!resource.is_empty(), "mutation policies name the resource");
        self.policy(resource)
            .and_then(|(_, mutation)| mutation)
            .ok_or_else(|| self.refusal(resource, "mutation"))?
            .authorize(subject, resource)
    }
}
