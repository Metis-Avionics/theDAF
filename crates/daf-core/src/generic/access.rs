//! Object-safe JSON boundary adapters over the typed domain contracts.

use async_trait::async_trait;
use serde_json::Value;
use sha2::Digest;
use std::sync::Arc;

use crate::DataAccessError;
use crate::generic::contracts::{MutationDomain, QueryDomain};
use crate::generic::types::{MutationContext, QueryInfo, Subject};

/// Transport-free JSON query/mutation boundary.
#[async_trait]
pub trait DataAccess: Send + Sync {
    /// Serve a JSON query.
    ///
    /// # Errors
    /// Returns DAF's structured failure taxonomy.
    async fn query(&self, info: &QueryInfo, subject: &Subject) -> Result<Value, DataAccessError>;

    /// Serve a JSON mutation.
    ///
    /// # Errors
    /// Returns DAF's structured failure taxonomy.
    async fn execute_mutation(
        &self,
        _info: &crate::generic::types::MutationInfo,
        _subject: &Subject,
        _context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        Err(DataAccessError::NotFound(crate::NotFoundError::new(
            "no mutation adapter registered for the requested resource",
        )))
    }
}

/// Reusable replay store for mutation outputs.
#[async_trait]
pub trait Idempotency: Send + Sync {
    /// Return a prior result, or `None` when no result is recorded.
    ///
    /// # Errors
    /// Propagates a store failure.
    async fn replay(&self, key: &str) -> Result<Option<Value>, DataAccessError>;
    /// Record a result for future replay.
    ///
    /// # Errors
    /// Propagates a store failure.
    async fn record(
        &self,
        key: &str,
        result: &Value,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), DataAccessError>;
}

/// Domain-owned post-scope pool gate with DAF's B-281 wording.
///
/// # Errors
/// Repository denial naming the requested resource when no pool was bound.
pub fn required_pool<'pool, P>(
    pool: Option<&'pool P>,
    resource: &'static str,
) -> Result<&'pool P, DataAccessError> {
    debug_assert!(!resource.is_empty(), "pool checks name the resource");
    pool.ok_or_else(|| pool_denial(resource))
}

fn pool_denial(resource: &str) -> DataAccessError {
    debug_assert!(!resource.is_empty(), "pool denials name the resource");
    DataAccessError::Repository(crate::RepositoryError::new(format!(
        "Postgres pool not configured; {resource} unavailable"
    )))
}

fn wrong_query_route(expected: &str, resource: &str) -> DataAccessError {
    debug_assert!(!expected.is_empty(), "query route names the domain");
    debug_assert!(!resource.is_empty(), "query route names the wrong resource");
    DataAccessError::Validation(crate::ValidationError::new(format!(
        "the registered query adapter serves '{expected}', not '{resource}'"
    )))
}

fn wrong_mutation_route(expected: &str, resource: &str) -> DataAccessError {
    debug_assert!(!expected.is_empty(), "mutation route names the domain");
    debug_assert!(
        !resource.is_empty(),
        "mutation route names the wrong resource"
    );
    DataAccessError::Validation(crate::ValidationError::new(format!(
        "the registered mutation adapter serves '{expected}', not '{resource}'"
    )))
}

/// Derive a deterministic subject-scoped key from canonical JSON input.
#[must_use]
pub fn subject_json_key<T: serde::Serialize>(subject: &Subject, input: &T) -> String {
    debug_assert!(
        std::mem::size_of::<u32>() == 4,
        "SHA-256 word size is 32-bit"
    );
    let encoded = serde_json::to_vec(input).unwrap_or_default();
    let mut hasher = ::sha2::Sha256::new();
    hasher.update(subject.identity_id.as_bytes());
    hasher.update([0]);
    hasher.update(subject.tenant_id.as_bytes());
    hasher.update([0]);
    hasher.update(&encoded);
    format!(
        "{}:{}",
        subject.identity_id,
        ::hex::encode(hasher.finalize())
    )
}

fn encode<T: serde::Serialize>(domain: &str, value: T) -> Result<Value, DataAccessError> {
    serde_json::to_value(value).map_err(|error| {
        DataAccessError::Internal(crate::InternalError::new(format!(
            "{domain} output encode: {error}"
        )))
    })
}

/// The object-safe JSON query boundary over one typed query domain.
#[derive(Debug, Clone, Default)]
pub struct QueryAccess<Q, P: Send + Sync + 'static> {
    domain: Q,
    pool: Option<P>,
}

impl<Q, P> QueryAccess<Q, P>
where
    P: Send + Sync + 'static,
{
    /// Compose a row with a live pool, or `None` for B-281's fail-closed row.
    #[must_use]
    pub const fn new(domain: Q, pool: Option<P>) -> Self {
        Self { domain, pool }
    }

    /// Access the stored domain (diagnostics/testing only).
    #[must_use]
    pub const fn domain(&self) -> &Q {
        &self.domain
    }

    /// Access the optional pool (diagnostics/testing only).
    #[must_use]
    pub const fn pool(&self) -> Option<&P> {
        self.pool.as_ref()
    }
}

#[async_trait]
impl<Q, P> DataAccess for QueryAccess<Q, P>
where
    Q: QueryDomain<P>,
    P: Send + Sync + 'static,
{
    async fn query(&self, info: &QueryInfo, subject: &Subject) -> Result<Value, DataAccessError> {
        debug_assert!(
            !info.resource.is_empty(),
            "the boundary must name every resource"
        );
        if info.resource != Q::RESOURCE {
            return Err(wrong_query_route(Q::RESOURCE, &info.resource));
        }
        let arguments = info.arguments.clone().unwrap_or_default();
        let input = Q::parse_arguments(&arguments).await?;
        let output = self.domain.run(self.pool.as_ref(), input, subject).await?;
        encode(Q::RESOURCE, output)
    }

    async fn execute_mutation(
        &self,
        info: &crate::generic::types::MutationInfo,
        _subject: &Subject,
        _context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        Err(DataAccessError::Authorization(
            crate::AuthorizationError::new(format!(
                "'{}' is a query — it serves no mutations",
                info.resource
            )),
        ))
    }
}

/// The object-safe JSON mutation boundary over one typed mutation domain.
#[derive(Clone, Default)]
pub struct MutationAccess<M, P: Send + Sync + 'static> {
    domain: M,
    pool: Option<P>,
    idempotency: Option<Arc<dyn Idempotency>>,
}

impl<M, P> MutationAccess<M, P>
where
    P: Send + Sync + 'static,
{
    /// Compose one write row over a live pool; `None` denies like every
    /// other pool-less deployment row.
    #[must_use]
    pub const fn new(domain: M, pool: Option<P>) -> Self {
        Self {
            domain,
            pool,
            idempotency: None,
        }
    }

    /// Attach an idempotency store used only when the domain derives a key.
    #[must_use]
    pub fn with_idempotency(mut self, store: Arc<dyn Idempotency>) -> Self {
        debug_assert!(
            std::mem::size_of::<Option<Arc<dyn Idempotency>>>() > 0,
            "idempotency slot is representable"
        );
        self.idempotency = Some(store);
        self
    }

    /// The domain behind the row (diagnostics/testing only).
    #[must_use]
    pub const fn domain(&self) -> &M {
        &self.domain
    }

    /// The optional pool behind the row (diagnostics/testing only).
    #[must_use]
    pub const fn pool(&self) -> Option<&P> {
        self.pool.as_ref()
    }
}

#[async_trait]
impl<M, P> DataAccess for MutationAccess<M, P>
where
    M: MutationDomain<P>,
    P: Send + Sync + 'static,
{
    async fn query(&self, info: &QueryInfo, _subject: &Subject) -> Result<Value, DataAccessError> {
        Err(DataAccessError::Validation(crate::ValidationError::new(
            format!("'{}' is a mutation — it serves no queries", info.resource),
        )))
    }

    async fn execute_mutation(
        &self,
        info: &crate::generic::types::MutationInfo,
        subject: &Subject,
        context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        if info.resource != M::RESOURCE {
            return Err(wrong_mutation_route(M::RESOURCE, &info.resource));
        }
        // Keep pool availability under domain scope/tenant validation
        // (0.3.1 ordering): DAF exposes the optional pool, but never
        // preempts the tenant rule — the domain function owns the B-281
        // denial and decides it after its own scope checks. Adapters only
        // expose pool availability.
        let input = M::parse_arguments(&info.arguments).await?;
        if let Some(idem) = self.idempotency.as_ref()
            && let Some(key) = self.domain.idempotency_key(subject, &input)
        {
            if let Some(replayed) = idem.replay(&key).await? {
                return Ok(replayed);
            }
            let output = self
                .domain
                .run(self.pool.as_ref(), input, subject, context)
                .await?;
            let encoded = encode(M::RESOURCE, &output)?;
            idem.record(&key, &encoded, None).await?;
            return Ok(encoded);
        }
        let output = self
            .domain
            .run(self.pool.as_ref(), input, subject, context)
            .await?;
        encode(M::RESOURCE, output)
    }
}

/// Convert a domain access value into one registry row without a call-site cast.
#[must_use]
pub fn typed_access_row(
    resource: &'static str,
    access: impl DataAccess + 'static,
) -> (&'static str, Arc<dyn DataAccess>) {
    debug_assert!(!resource.is_empty(), "registry rows name their resource");
    (resource, Arc::new(access))
}

/// A resource-addressed table of object-safe access rows.
#[derive(Clone, Default)]
pub struct DomainDataAccess {
    rows: std::vec::Vec<(&'static str, Arc<dyn DataAccess>)>,
}

impl DomainDataAccess {
    /// Compose rows; later duplicate rows must not happen.
    #[must_use]
    pub fn new(rows: std::vec::Vec<(&'static str, Arc<dyn DataAccess>)>) -> Self {
        debug_assert!(
            !rows.iter().any(|(resource, _)| resource.is_empty()),
            "registry resources must be named"
        );
        for (index, resource) in rows.iter().map(|(resource, _)| *resource).enumerate() {
            debug_assert!(
                rows[index + 1..]
                    .iter()
                    .all(|(other, _)| *other != resource),
                "duplicate resource row"
            );
        }
        Self { rows }
    }

    /// Find the object-safe row for a resource.
    #[must_use]
    pub fn row(&self, resource: &str) -> Option<&Arc<dyn DataAccess>> {
        debug_assert!(!resource.is_empty(), "row searches name the resource");
        self.rows.iter().find_map(|(candidate, access)| {
            if *candidate == resource {
                Some(access)
            } else {
                None
            }
        })
    }
}

#[async_trait]
impl DataAccess for DomainDataAccess {
    async fn query(&self, info: &QueryInfo, subject: &Subject) -> Result<Value, DataAccessError> {
        debug_assert!(!info.resource.is_empty(), "resource must be named");
        let Some(access) = self.row(&info.resource) else {
            return Err(DataAccessError::Authorization(
                crate::AuthorizationError::new(format!(
                    "resource '{}' is not served by this policy",
                    info.resource
                )),
            ));
        };
        access.query(info, subject).await
    }

    async fn execute_mutation(
        &self,
        info: &crate::generic::types::MutationInfo,
        subject: &Subject,
        context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        let Some(access) = self.row(&info.resource) else {
            return Err(DataAccessError::Authorization(
                crate::AuthorizationError::new(format!(
                    "resource '{}' is not served by this policy",
                    info.resource
                )),
            ));
        };
        access.execute_mutation(info, subject, context).await
    }
}
