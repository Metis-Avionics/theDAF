#![allow(clippy::unwrap_used, clippy::expect_used)]
use async_trait::async_trait;
use daf_core::daf_policy;
use daf_core::generic::{
    DataAccess, MutationContext, MutationInfo, PolicyAccess, QueryInfo, Subject,
};
use daf_core::{DataAccessError, RepositoryError, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EchoInput {
    value: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct EchoAccess;

#[async_trait]
impl DataAccess for EchoAccess {
    async fn query(&self, info: &QueryInfo, subject: &Subject) -> Result<Value, DataAccessError> {
        if info.resource != "echo" {
            return Err(DataAccessError::Validation(ValidationError::new(format!(
                "the registered query adapter serves 'echo', not '{}'",
                info.resource
            ))));
        }
        let input = serde_json::from_value::<EchoInput>(info.arguments.clone().unwrap_or_default())
            .map_err(|e| {
                DataAccessError::Validation(ValidationError::new(format!("invalid arguments: {e}")))
            })?;
        Ok(json!({"value": input.value, "caller": subject.identity_id}))
    }

    async fn execute_mutation(
        &self,
        info: &MutationInfo,
        _subject: &Subject,
        _context: &MutationContext,
    ) -> Result<Value, DataAccessError> {
        Err(DataAccessError::Repository(RepositoryError::new(format!(
            "Postgres pool not configured; {} unavailable",
            info.resource
        ))))
    }
}

daf_policy! {
    policy struct EchoPolicy;
    query { "echo" => Authenticated }
    mutation { "write" => GlobalAdmin }
}

#[tokio::test]
async fn policy_admits_before_the_adapter() {
    let access = PolicyAccess::<EchoAccess, EchoPolicy>::new(EchoAccess);
    let result = access
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!({"value": 8})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap();
    assert_eq!(result["value"], 8);
}

#[tokio::test]
async fn policy_refuses_before_the_adapter() {
    let access = PolicyAccess::<EchoAccess, EchoPolicy>::new(EchoAccess);
    let error = access
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!({"value": 8})),
            },
            &Subject::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Authorization(_)));
}

#[tokio::test]
async fn malformed_arguments_fail_after_policy() {
    let access = PolicyAccess::<EchoAccess, EchoPolicy>::new(EchoAccess);
    let error = access
        .query(
            &QueryInfo {
                resource: "echo".to_string(),
                arguments: Some(json!("bad")),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Validation(_)));
}
