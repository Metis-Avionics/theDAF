//! Generic DAF contracts for every typed domain.
//!
//! The typed plane is transport- and storage-blind. Adapters at its edge
//! perform resource routing, JSON parsing and B-281 pool denial; SQL and
//! domain compute stay with domain-owned functions.
pub mod access;
pub mod authorized;
pub mod authorizer;
pub mod contracts;
pub mod macros;
pub mod policy;
pub mod types;

pub use access::subject_json_key;
pub use access::{
    DataAccess, DomainDataAccess, Idempotency, MutationAccess, QueryAccess, typed_access_row,
};
pub use authorized::PolicyAccess;
pub use authorizer::{PolicyTable, StaticPolicyAuthorizer};
pub use contracts::{MutationDomain, QueryDomain};
pub use policy::{AdmissionMode, ResourcePolicy};
pub use types::{MutationContext, MutationInfo, QueryInfo, Subject};
