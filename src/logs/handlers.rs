use super::*;
use crate::Message;
use crate::auth::jwt::Claims;
use crate::logs::dto::GetActivityLogsQuery;
use crate::logs::model::ActivityLog;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router, middleware};
use std::sync::Arc;
use tower::ServiceBuilder;
use uuid::Uuid;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/logs", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(get_logs))
        .route("/{log_id}", get(get_log_by_id))
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::admin_authorize,
        )))
        .with_state(state)
}

/// Retrieve activity logs (Admin only)
///
/// Fetches audit trail logs with filtering by user, activity type, target, and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    get,
    path = "/logs",
    params(
        GetActivityLogsQuery
    ),
    responses(
        (status = 200, description = "Logs retrieved successfully", body = Message<Vec<ActivityLog>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
    ),
    tag = "Activity Logs"
)]
pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    _claims: Claims,
    Query(query): Query<GetActivityLogsQuery>,
) -> Result<Json<Message<Vec<ActivityLog>>>, ModuleError> {
    let logs = services::get_logs(state.pool.clone(), query).await?;
    let response = Message::context("Logs retrieved successfully.", logs);
    Ok(Json(response))
}

/// Get single activity log by ID (Admin only)
///
/// Retrieves complete details of an activity log record.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    get,
    path = "/logs/{log_id}",
    params(
        ("log_id" = Uuid, Path, description = "Unique UUID of the activity log")
    ),
    responses(
        (status = 200, description = "Log retrieved successfully", body = Message<ActivityLog>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Log not found")
    ),
    tag = "Activity Logs"
)]
pub async fn get_log_by_id(
    State(state): State<Arc<AppState>>,
    _claims: Claims,
    Path(log_id): Path<Uuid>,
) -> Result<Json<Message<ActivityLog>>, ModuleError> {
    let log = services::get_log_by_id(state.pool.clone(), log_id).await?;
    let response = Message::context("Log retrieved successfully.", log);
    Ok(Json(response))
}
