#![allow(clippy::unwrap_used, clippy::expect_used)]
use async_trait::async_trait;
use daf_core::generic::{
    DataAccess, Idempotency, MutationContext, MutationDomain, QueryDomain,
    QueryInfo as TypedQueryInfo, Subject,
};
use daf_core::{DataAccessError, daf_mutation, daf_query};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroInput {
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MacroOutput {
    pub value: i64,
    pub caller: String,
}

daf_query! {
    pub access MacroQueryAccess,
    pub domain MacroQueryDomain,
    resource = "macro_query",
    pool = (),
    input = MacroInput,
    output = MacroOutput,
    run(pool, input, subject) {
        let _pool = daf_core::generic::required_pool(pool, "macro_query")?;
        Ok(MacroOutput {
            value: input.value,
            caller: subject.identity_id.clone(),
        })
    }
}

daf_mutation! {
    pub access MacroMutationAccess,
    pub domain MacroMutationDomain,
    resource = "macro_mutation",
    pool = (),
    input = MacroInput,
    output = MacroOutput,
    idempotency = none,
    run(pool, input, subject, _context) {
        let _pool = daf_core::generic::required_pool(pool, "macro_mutation")?;
        Ok(MacroOutput {
            value: input.value,
            caller: subject.identity_id.clone(),
        })
    }
}

daf_mutation! {
    pub access ReplayedAccess,
    pub domain ReplayedDomain,
    resource = "macro_replay",
    pool = (),
    input = MacroInput,
    output = MacroOutput,
    idempotency = subject_and_input,
    run(pool, input, subject, _context) {
        let _pool = daf_core::generic::required_pool(pool, "macro_mutation")?;
        Ok(MacroOutput {
            value: input.value,
            caller: subject.identity_id.clone(),
        })
    }
}

#[tokio::test]
async fn macro_query_expands_the_typed_contract() {
    assert_eq!(
        <MacroQueryDomain as QueryDomain<()>>::RESOURCE,
        "macro_query"
    );
    assert_eq!(
        <MacroQueryDomain as QueryDomain<()>>::RESOURCE,
        "macro_query"
    );
    let subject = Subject::new("caller", "tenant");
    let error = MacroQueryAccess::new(Some(()))
        .query(
            &TypedQueryInfo {
                resource: "macro_query".to_string(),
                arguments: None,
            },
            &subject,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DataAccessError::Validation(_)));

    let error = MacroQueryAccess::new(Some(()))
        .query(
            &TypedQueryInfo {
                resource: "macro_query".to_string(),
                arguments: Some(json!({"value": 7})),
            },
            &subject,
        )
        .await
        .unwrap();
    assert_eq!(error["value"], 7);
}

#[tokio::test]
async fn macro_mutation_has_the_typed_domain_and_b281_denial() {
    assert_eq!(
        <MacroMutationDomain as MutationDomain<()>>::RESOURCE,
        "macro_mutation"
    );
    let error = MacroMutationAccess::new(None)
        .execute_mutation(
            &daf_core::generic::MutationInfo {
                resource: "macro_mutation".to_string(),
                arguments: json!({"value": 7}),
            },
            &Subject::new("caller", "tenant"),
            &MutationContext::new(),
        )
        .await
        .unwrap_err();
    let DataAccessError::Repository(message) = error else {
        panic!("expected B-281 repository denial");
    };
    assert_eq!(
        message.0,
        "Postgres pool not configured; macro_mutation unavailable"
    );
}

#[tokio::test]
async fn wrong_resource_by_a_concrete_macro_adapter_validation_refuses() {
    let access = MacroQueryAccess::new(Some(()));
    let error = access
        .query(
            &TypedQueryInfo {
                resource: "other".to_string(),
                arguments: Some(json!({"value": 1})),
            },
            &Subject::new("caller", "tenant"),
        )
        .await
        .unwrap_err();
    let DataAccessError::Validation(message) = error else {
        panic!("expected validation refusal");
    };
    assert_eq!(
        message.message,
        "the registered query adapter serves 'macro_query', not 'other'"
    );
}

#[derive(Debug)]
struct MemoryIdempotency {
    stored: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

#[async_trait]
impl Idempotency for MemoryIdempotency {
    async fn replay(&self, key: &str) -> Result<Option<serde_json::Value>, DataAccessError> {
        Ok(self.stored.lock().unwrap().get(key).cloned())
    }

    async fn record(
        &self,
        key: &str,
        result: &serde_json::Value,
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
async fn macro_subject_and_input_replays_the_serialized_result() {
    let store = std::sync::Arc::new(MemoryIdempotency {
        stored: std::sync::Mutex::new(std::collections::HashMap::new()),
    });
    let access =
        ReplayedAccess::new(Some(())).with_idempotency(store as std::sync::Arc<dyn Idempotency>);
    let subject = Subject::new("caller", "tenant");
    let info = daf_core::generic::MutationInfo {
        resource: "macro_replay".to_string(),
        arguments: json!({"value": 13}),
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
    assert_eq!(first["value"], 13);
}
