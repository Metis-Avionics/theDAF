use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;

use daf_algorithms::FibonacciDP;
use daf_application::DataAccess;
use daf_cache::{CacheManager, CachelitoCache, MokaCache};
use daf_core::{DeleteInfo, JsonValue, PostInfo, PutInfo, QueryInfo, ResourceId};
use daf_repository::MemoryRepository;

#[derive(serde::Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_hit: Option<bool>,
    success: bool,
}

#[derive(serde::Serialize)]
struct ErrResponse {
    ok: bool,
    error: String,
}

impl From<ErrResponse> for OkResponse {
    fn from(e: ErrResponse) -> Self {
        OkResponse {
            ok: false,
            success: false,
            error: Some(e.error),
            error_type: Some("internal".to_string()),
            data: None,
            resource_id: None,
            cache_hit: None,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(tag = "op")]
enum Command {
    #[serde(rename = "post")]
    Post {
        resource_type: String,
        data: HashMap<String, JsonValue>,
    },
    #[serde(rename = "put")]
    Put {
        resource_id: String,
        data: HashMap<String, JsonValue>,
    },
    #[serde(rename = "delete")]
    Delete { resource_id: String },
    #[serde(rename = "query")]
    Query {
        resource_id: String,
        filters: Option<HashMap<String, JsonValue>>,
        algorithm: Option<String>,
    },
}

struct ParityState {
    daf: DataAccess,
    _repo: Arc<dyn daf_core::Repository<JsonValue>>,
}

impl ParityState {
    fn new() -> Self {
        let algorithms: HashMap<String, Arc<dyn daf_core::Algorithm>> = HashMap::from([(
            "fib".to_string(),
            Arc::new(FibonacciDP::new()) as Arc<dyn daf_core::Algorithm>,
        )]);
        let repo: Arc<dyn daf_core::Repository<JsonValue>> = Arc::new(MemoryRepository::new());
        let cache = Arc::new({
            let l1 = CachelitoCache::new();
            let l2 = MokaCache::new(1024);
            CacheManager::new(l1, l2, None, None)
        });
        let daf = DataAccess::new(repo.clone(), cache, Some(algorithms), None);
        Self { daf, _repo: repo }
    }

    async fn execute(&self, cmd: Command) -> OkResponse {
        debug_assert!(
            matches!(
                cmd,
                Command::Post { .. }
                    | Command::Put { .. }
                    | Command::Delete { .. }
                    | Command::Query { .. }
            ),
            "cmd must be a valid Command variant"
        );
        match cmd {
            Command::Post {
                resource_type,
                data,
            } => self.handle_post(resource_type, data).await,
            Command::Put { resource_id, data } => self.handle_put(resource_id, data).await,
            Command::Delete { resource_id } => self.handle_delete(resource_id).await,
            Command::Query {
                resource_id,
                filters,
                algorithm,
            } => self.handle_query(resource_id, filters, algorithm).await,
        }
    }

    async fn handle_post(
        &self,
        resource_type: String,
        data: HashMap<String, JsonValue>,
    ) -> OkResponse {
        match self
            .daf
            .post(
                PostInfo {
                    resource_type,
                    data,
                },
                None,
            )
            .await
        {
            Ok(r) => OkResponse {
                ok: true,
                success: r.success,
                resource_id: r.resource_id.map(|rid| rid.0),
                data: r.data,
                error: r.error,
                error_type: r.error_type,
                cache_hit: None,
            },
            Err(e) => ErrResponse {
                ok: false,
                error: e.to_string(),
            }
            .into(),
        }
    }

    async fn handle_put(
        &self,
        resource_id: String,
        data: HashMap<String, JsonValue>,
    ) -> OkResponse {
        match self
            .daf
            .put(
                PutInfo {
                    resource_id: ResourceId::new(resource_id),
                    data,
                },
                None,
            )
            .await
        {
            Ok(r) => OkResponse {
                ok: true,
                success: r.success,
                resource_id: r.resource_id.map(|rid| rid.0),
                data: r.data,
                error: r.error,
                error_type: r.error_type,
                cache_hit: None,
            },
            Err(e) => ErrResponse {
                ok: false,
                error: e.to_string(),
            }
            .into(),
        }
    }

    async fn handle_delete(&self, resource_id: String) -> OkResponse {
        match self
            .daf
            .delete(
                DeleteInfo {
                    resource_id: ResourceId::new(resource_id),
                },
                None,
            )
            .await
        {
            Ok(r) => OkResponse {
                ok: true,
                success: r.success,
                resource_id: r.resource_id.map(|rid| rid.0),
                data: r.data,
                error: r.error,
                error_type: r.error_type,
                cache_hit: None,
            },
            Err(e) => ErrResponse {
                ok: false,
                error: e.to_string(),
            }
            .into(),
        }
    }

    async fn handle_query(
        &self,
        resource_id: String,
        filters: Option<HashMap<String, JsonValue>>,
        algorithm: Option<String>,
    ) -> OkResponse {
        match self
            .daf
            .query(
                QueryInfo {
                    resource_id: ResourceId::new(resource_id),
                    filters,
                    algorithm,
                },
                None,
            )
            .await
        {
            Ok(r) => OkResponse {
                ok: true,
                success: r.success,
                resource_id: None,
                data: r.data,
                error: r.error,
                error_type: r.error_type,
                cache_hit: Some(r.cache_hit),
            },
            Err(e) => ErrResponse {
                ok: false,
                error: e.to_string(),
            }
            .into(),
        }
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let state = ParityState::new();
    let reader = std::io::BufReader::new(std::io::stdin().lock());
    let mut stdout = std::io::stdout().lock();

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let cmd: Command = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                let response = OkResponse {
                    ok: false,
                    success: false,
                    error: Some(format!("parse error: {e}")),
                    error_type: Some("parse".to_string()),
                    data: None,
                    resource_id: None,
                    cache_hit: None,
                };
                let json = serde_json::to_string(&response).unwrap();
                writeln!(stdout, "{json}").ok();
                stdout.flush().ok();
                continue;
            }
        };
        let result = rt.block_on(state.execute(cmd));
        let json = serde_json::to_string(&result).unwrap();
        writeln!(stdout, "{json}").ok();
        stdout.flush().ok();
    }
}
