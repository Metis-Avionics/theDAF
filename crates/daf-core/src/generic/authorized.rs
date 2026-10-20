//! Admission-policy composition for JSON access rows.

use async_trait::async_trait;
use serde_json::Value;

use crate::DataAccessError;
use crate::generic::access::DataAccess;
use crate::generic::authorizer::{PolicyTable, StaticPolicyAuthorizer};
use crate::generic::types::{MutationContext, MutationInfo, QueryInfo, Subject};

/// An access row gated once by a static policy table before the adapter.
#[derive(Debug, Clone)]
pub struct PolicyAccess<A, T> {
    access: A,
    policy: StaticPolicyAuthorizer<T>,
}

impl<A, T> PolicyAccess<A, T>
where
    T: PolicyTable + 'static,
{
    /// Compose an adapter with its static policy.
    #[must_use]
    pub const fn new(access: A) -> Self {
        Self {
            access,
            policy: StaticPolicyAuthorizer::new(),
        }
    }

    /// The adapter behind the policy (diagnostics/testing only).
    #[must_use]
    pub const fn access(&self) -> &A {
        &self.access
    }
}

#[async_trait]
impl<A, T> DataAccess for PolicyAccess<A, T>
where
    A: DataAccess,
    T: PolicyTable + 'static,
{
    async fn query(&self, info: &QueryInfo, subject: &Subject) -> Result<Value, DataAccessError> {
        debug_assert!(!info.resource.is_empty(), "query policies name resources");
        self.policy.authorize_query(subject, &info.resource)?;
        self.access.query(info, subject).await
    }

    async fn execute_mutation(
        &self,
        info: &MutationInfo,
        subject: &Subject,
        context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        debug_assert!(!info.resource.is_empty(), "policy rows name resources");
        self.policy.authorize_mutation(subject, &info.resource)?;
        self.access.execute_mutation(info, subject, context).await
    }
}

/// Compose a policy around any [`DataAccess`].
#[must_use]
pub const fn with_policy<A, T>(access: A) -> PolicyAccess<A, T>
where
    A: DataAccess,
    T: PolicyTable + 'static,
{
    PolicyAccess::new(access)
}
