use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};

use daf_application::DataAccess;
use daf_core::{
    DataAccessError, DeleteInfo, MutationResult, PostInfo, PutInfo, QueryInfo, QueryResult,
    ResourceId, UserId,
};

type CurrentUserFn = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Option<UserId>, DataAccessError>> + Send>>
        + Send
        + Sync,
>;

struct AppState {
    data_access: Arc<DataAccess>,
    get_current_user: CurrentUserFn,
}

#[derive(Debug)]
pub struct AppError(DataAccessError);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let DataAccessError::Authorization(_) = &self.0 {
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }
        if let DataAccessError::NotFound(_) = &self.0 {
            return (StatusCode::NOT_FOUND, "Not Found").into_response();
        }
        if let DataAccessError::Validation(_) = &self.0 {
            return (StatusCode::BAD_REQUEST, self.0.to_string()).into_response();
        }
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<DataAccessError>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

pub struct DataAccessRouter {
    router: Router,
}

impl DataAccessRouter {
    pub fn new<F, Fut>(data_access: Arc<DataAccess>, get_current_user: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Option<UserId>, DataAccessError>> + Send + 'static,
    {
        let state = Arc::new(AppState {
            data_access,
            get_current_user: Arc::new(move || Box::pin(get_current_user())),
        });

        let router = Router::new()
            .route("/query/{resource_id}", get(query_handler))
            .route("/data", post(post_handler))
            .route(
                "/data/{resource_id}",
                put(put_handler).delete(delete_handler),
            )
            .with_state(state);

        Self { router }
    }

    pub fn into_router(self) -> Router {
        self.router
    }
}

async fn query_handler(
    State(state): State<Arc<AppState>>,
    Path(resource_id): Path<String>,
) -> Result<Json<QueryResult>, AppError> {
    let user = (state.get_current_user)().await?;
    let info = QueryInfo {
        resource_id: ResourceId::new(resource_id),
        filters: None,
        algorithm: None,
    };
    let result = state.data_access.query(info, user.as_ref()).await?;
    Ok(Json(result))
}

async fn post_handler(
    State(state): State<Arc<AppState>>,
    Json(info): Json<PostInfo>,
) -> Result<Json<MutationResult>, AppError> {
    let user = (state.get_current_user)().await?;
    let result = state.data_access.post(info, user.as_ref()).await?;
    Ok(Json(result))
}

async fn put_handler(
    State(state): State<Arc<AppState>>,
    Path(resource_id): Path<String>,
    Json(mut info): Json<PutInfo>,
) -> Result<Json<MutationResult>, AppError> {
    let user = (state.get_current_user)().await?;
    info.resource_id = ResourceId::new(resource_id);
    let result = state.data_access.put(info, user.as_ref()).await?;
    Ok(Json(result))
}

async fn delete_handler(
    State(state): State<Arc<AppState>>,
    Path(resource_id): Path<String>,
) -> Result<Json<MutationResult>, AppError> {
    let user = (state.get_current_user)().await?;
    let info = DeleteInfo {
        resource_id: ResourceId::new(resource_id),
    };
    let result = state.data_access.delete(info, user.as_ref()).await?;
    Ok(Json(result))
}
