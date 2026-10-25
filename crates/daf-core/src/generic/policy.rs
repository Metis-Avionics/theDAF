//! Static, declarative admission policy.

use crate::DataAccessError;
use crate::generic::types::Subject;

/// The four admission modes supported by the static policy table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum AdmissionMode {
    /// Any boundary call, including one without a verified subject.
    Public,
    /// Any verified non-empty identity.
    Authenticated,
    /// A verified subject with a tenant scope; global admins may cross.
    Tenant,
    /// Only an explicitly flagged global administrator.
    GlobalAdmin,
}

impl AdmissionMode {
    /// Apply the mode independently of transport context.
    ///
    /// # Errors
    /// Returns [`DataAccessError::Authorization`] for an unmet subject shape.
    pub fn authorize(&self, subject: &Subject, resource: &str) -> Result<(), DataAccessError> {
        debug_assert!(
            !resource.is_empty(),
            "policy evaluation requires a named resource"
        );
        match self {
            Self::Public => Ok(()),
            Self::Authenticated if !subject.identity_id.is_empty() => Ok(()),
            Self::Tenant if !subject.identity_id.is_empty() && !subject.tenant_id.is_empty() => {
                Ok(())
            }
            Self::GlobalAdmin if subject.is_global_admin => Ok(()),
            Self::Authenticated => Err(DataAccessError::Authorization(
                crate::AuthorizationError::new(format!(
                    "{resource} requires an authenticated subject"
                )),
            )),
            Self::Tenant => Err(DataAccessError::Authorization(
                crate::AuthorizationError::new(format!("{resource} requires a tenant subject")),
            )),
            Self::GlobalAdmin => Err(DataAccessError::Authorization(
                crate::AuthorizationError::new(format!(
                    "{resource} requires global-admin authorization"
                )),
            )),
        }
    }
}

/// One operation's static admission row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourcePolicy {
    /// Stable resource name.
    pub resource: &'static str,
    /// Query admission mode, or `None` when the read is not served.
    pub query: Option<AdmissionMode>,
    /// Mutation admission mode, or `None` when the write is not served.
    pub mutation: Option<AdmissionMode>,
}
