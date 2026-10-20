#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use daf_core::generic::{
    DataAccess, DomainDataAccess, Idempotency as GenericIdempotency, MutationAccess,
    MutationContext, MutationDomain, MutationInfo, QueryAccess, QueryDomain, QueryInfo,
    StaticPolicyAuthorizer, Subject,
};
use daf_core::{DataAccessError, daf_row, daf_rows};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EchoInput {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EchoOutput {
    pub value: i64,
    pub caller: String,
}

#[derive(Debug, Clone, Default)]
pub struct EchoQueryDomain;

#[async_trait]
impl QueryDomain<()> for EchoQueryDomain {
    const RESOURCE: &'static str = "echo";
    type Input = EchoInput;
    type Output = EchoOutput;

    async fn parse_arguments(arguments: &Value) -> Result<Self::Input, DataAccessError> {
        serde_json::from_value(arguments.clone()).map_err(|e| {
            DataAccessError::Validation(daf_core::ValidationError::new(format!(
                "invalid arguments: {e}"
            )))
        })
    }

    async fn run(
        &self,
        _pool: &(),
        input: Self::Input,
        subject: &Subject,
    ) -> Result<Self::Output, DataAccessError> {
        Ok(EchoOutput {
            value: input.value,
            caller: subject.identity_id.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteInput {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteOutput {
    pub value: i64,
    pub writes: u64,
}

#[derive(Debug, Default)]
pub struct State {
    writes: Mutex<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct EchoMutationDomain {
    state: std::sync::Arc<State>,
}

#[async_trait]
impl MutationDomain<String> for EchoMutationDomain {
    const RESOURCE: &'static str = "write";
    type Input = WriteInput;
    type Output = WriteOutput;

    async fn parse_arguments(arguments: &Value) -> Result<Self::Input, DataAccessError> {
        serde_json::from_value(arguments.clone()).map_err(|e| {
            DataAccessError::Validation(daf_core::ValidationError::new(format!(
                "invalid arguments: {e}"
            )))
        })
    }

    fn idempotency_key(&self, subject: &Subject, input: &Self::Input) -> Option<String> {
        Some(format!("{}/{}", subject.identity_id, input.value))
    }

    async fn run(
        &self,
        pool: &String,
        input: Self::Input,
        _subject: &Subject,
        _context: &MutationContext,
    ) -> Result<Self::Output, DataAccessError> {
        if pool.is_empty() {
            return Err(DataAccessError::Repository(daf_core::RepositoryError::new(
                "pool refused",
            )));
        }
        let writes = {
            let mut state = self.state.writes.lock().unwrap();
            *state += 1;
            *state
        };
        Ok(WriteOutput {
            value: input.value,
            writes,
        })
    }
}

#[derive(Debug, Default)]
struct MemoryIdempotency {
    stored: Mutex<std::collections::HashMap<String, Value>>,
}

#[async_trait]
impl GenericIdempotency for MemoryIdempotency {
    async fn replay(&self, key: &str) -> Result<Option<Value>, DataAccessError> {
        Ok(self.stored.lock().unwrap().get(key).cloned())
    }

    async fn record(
        &self,
        key: &str,
        result: &Value,
        _ttl: Option<std::time::Duration>,
    ) -> Result<(), DataAccessError> {
        self.stored
            .lock()
            .unwrap()
            .insert(key.to_string(), result.clone());
        Ok(())
    }
}

#[tokio::test]
async fn typed_query_parses_runs_and_encodes() {
    let access = QueryAccess::new(EchoQueryDomain, Some(()));
    let subject = Subject::new("caller", "tenant");
    let result = access
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!({"value": 42})),
            },
            &subject,
        )
        .await
        .unwrap();
    assert_eq!(result["value"], 42);
    assert_eq!(result["caller"], "caller");
}

#[tokio::test]
async fn query_access_denies_missing_pool_by_resource() {
    let access = QueryAccess::<EchoQueryDomain, ()>::new(EchoQueryDomain, None);
    let error = access
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!({"value": 1})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap_err();
    let DataAccessError::Repository(message) = error else {
        panic!("expected repository error");
    };
    assert_eq!(message.0, "Postgres pool not configured; echo unavailable");
}

#[tokio::test]
async fn query_access_rejects_wrong_resource() {
    let access = QueryAccess::new(EchoQueryDomain, Some(()));
    let error = access
        .query(
            &QueryInfo {
                resource: "other".to_string(),
                arguments: Some(json!({"value": 1})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Validation(_)));
}

#[tokio::test]
async fn mutation_replays_without_a_second_side_effect() {
    let store = std::sync::Arc::new(MemoryIdempotency::default());
    let domain = EchoMutationDomain::default();
    let access =
        MutationAccess::new(domain, Some("pool".to_string())).with_idempotency(store.clone());
    let subject = Subject::new("caller", "tenant");
    let info = MutationInfo {
        resource: "write".to_string(),
        arguments: json!({"value": 7}),
    };
    let first = access
        .execute_mutation(&info, &subject, &MutationContext::new())
        .await
        .unwrap();
    let second = access
        .execute_mutation(&info, &subject, &MutationContext::new())
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first["writes"], 1);
}

#[tokio::test]
async fn denied_mutation_never_advances_written_count() {
    let domain = EchoMutationDomain::default();
    let access = MutationAccess::<_, String>::new(domain.clone(), None);
    let error = access
        .execute_mutation(
            &MutationInfo {
                resource: "write".to_string(),
                arguments: json!({"value": 7}),
            },
            &Subject::new("caller", "tenant"),
            &MutationContext::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Repository(_)));
    assert_eq!(*domain.state.writes.lock().unwrap(), 0);
}

#[tokio::test]
async fn domain_registry_refuses_unserved_resources() {
    let rows = vec![daf_row!(
        "echo",
        access = QueryAccess::new(EchoQueryDomain, Some(()))
    )];
    let registry = DomainDataAccess::new(rows);
    let error = registry
        .query(
            &QueryInfo {
                resource: "other".to_string(),
                arguments: Some(json!({"value": 1})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Authorization(_)));
}

#[tokio::test]
async fn static_policy_admits_and_enforces() {
    use daf_core::daf_policy;

    daf_policy! {
        policy struct EchoPolicy;
        query { "echo" => Authenticated }
        mutation { "write" => GlobalAdmin }
    }
    let policy = StaticPolicyAuthorizer::<EchoPolicy>::new();
    policy
        .authorize_query(&Subject::new("caller", "tenant"), "echo")
        .unwrap();
    let error = policy
        .authorize_query(&Subject::new("", ""), "echo")
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Authorization(_)));

    let admin = Subject::new("admin", "tenant").with_global_admin(true);
    policy.authorize_mutation(&admin, "write").unwrap();
    let error = policy
        .authorize_mutation(&Subject::new("member", "tenant"), "write")
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Authorization(_)));
}

#[tokio::test]
async fn daf_rows_composes_the_object_safe_row() {
    daf_rows! {
        pub fn query_rows(pool: Option<()>) ->
            Vec<(&'static str, std::sync::Arc<dyn daf_core::generic::DataAccess>)>;
        row "echo" => QueryAccess::new(EchoQueryDomain, pool)
    };
    let rows = query_rows(Some(()));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "echo");
    let result = rows[0]
        .1
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!({"value": 4})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap();
    assert_eq!(result["value"], 4);
}
