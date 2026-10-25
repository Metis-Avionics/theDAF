//! Typed, parametric DAF contracts shared by every domain port.

#![allow(deprecated)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The verified authorization subject crossing the DAF boundary.
///
/// DAF keeps identity, tenant, roles and the administrator flag as data so
/// adapters cannot reconstruct a caller from transport state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Subject {
    /// Stable authenticated identity.
    pub identity_id: String,
    /// Effective tenant/client scope.
    pub tenant_id: String,
    /// Role names used by policy evaluation.
    pub roles: Vec<String>,
    /// Explicit global-administrator flag.
    pub is_global_admin: bool,
}

impl Subject {
    /// Build a new subject with no roles and no administrator flag.
    #[must_use]
    pub fn new(identity_id: impl Into<String>, tenant_id: impl Into<String>) -> Self {
        let identity_id = identity_id.into();
        let tenant_id = tenant_id.into();
        Self {
            identity_id,
            tenant_id,
            roles: Vec::new(),
            is_global_admin: false,
        }
    }

    /// Add a role.
    #[must_use]
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        let role = role.into();
        debug_assert!(!role.is_empty(), "a role name cannot be empty");
        self.roles.push(role);
        self
    }

    /// Set the administrator flag.
    #[must_use]
    pub const fn with_global_admin(mut self, is_global_admin: bool) -> Self {
        self.is_global_admin = is_global_admin;
        self
    }

    /// `true` when the subject can reach the named tenant.
    #[must_use]
    pub fn belongs_to_tenant(&self, tenant_id: &str) -> bool {
        debug_assert!(!tenant_id.is_empty(), "tenant comparisons name a tenant");
        self.is_global_admin || self.tenant_id == tenant_id
    }
}

/// Convert a legacy 0.2 user identifier into a tenant-scoped generic subject.
///
/// This is the narrow compatibility bridge for 0.2 consumers. It makes the
/// legacy user identity explicit and requires the tenant at the conversion
/// site.
///
/// # Errors
/// Returns validation when the bridge would create an empty tenant.
pub fn subject_from_legacy_user(
    user: &crate::UserId,
    tenant_id: impl Into<String>,
) -> Result<Subject, crate::DataAccessError> {
    let tenant_id = tenant_id.into();
    if tenant_id.is_empty() {
        return Err(crate::DataAccessError::Validation(
            crate::ValidationError::new("legacy user admission requires a tenant"),
        ));
    }
    Ok(Subject {
        identity_id: user.0.clone(),
        tenant_id,
        roles: Vec::new(),
        is_global_admin: false,
    })
}

/// Transport-independent query request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QueryInfo {
    /// Stable resource name.
    pub resource: String,
    /// Optional JSON arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// A typed mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MutationInfo {
    /// Stable resource name.
    pub resource: String,
    /// JSON arguments.
    pub arguments: Value,
}

/// Capability context passed to mutation handlers.
#[derive(Clone, Default)]
pub struct MutationContext {
    capabilities: std::vec::Vec<(String, std::sync::Arc<dyn std::any::Any + Send + Sync>)>,
}

impl MutationContext {
    /// Create an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a named capability.
    pub fn insert<T: std::any::Any + Send + Sync>(
        &mut self,
        name: impl Into<String>,
        value: std::sync::Arc<T>,
    ) {
        let name = name.into();
        debug_assert!(!name.is_empty(), "capabilities are named");
        self.capabilities.push((name, value));
    }

    /// Type-check and clone a named capability.
    ///
    /// # Errors
    /// Fails closed when the capability is missing or has the wrong type.
    pub fn capability<T: std::any::Any + Send + Sync>(
        &self,
        name: &str,
    ) -> Result<std::sync::Arc<T>, crate::DataAccessError> {
        let missing = || {
            crate::DataAccessError::Internal(crate::InternalError::new(format!(
                "capability '{name}' is not wired into the mutation context"
            )))
        };
        for (key, value) in &self.capabilities {
            if key == name {
                return value.clone().downcast::<T>().map_err(|_| missing());
            }
        }
        Err(missing())
    }
}

impl std::fmt::Debug for MutationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Only names are emitted; capability payloads are never exposed.
        let names = self
            .capabilities
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>();
        f.debug_struct("MutationContext")
            .field("capabilities", &names)
            .finish()
    }
}
