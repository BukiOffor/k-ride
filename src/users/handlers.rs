use axum::http::HeaderMap;
use axum::middleware;
use tower::ServiceBuilder;

use super::services;
use super::*;

use crate::auth::jwt::Claims;
use crate::logs::{ActivityLog, ActivityType};

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/users", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/admin", post(create_user))
        .route("/{user_id}/role", patch(assign_role_to_user))
        .route(
            "/{user_id}/activate",
            patch(activate_user).post(activate_user),
        )
        .route(
            "/{user_id}/deactivate",
            patch(deactivate_user).post(deactivate_user),
        )
        .route("/", get(get_users))
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::admin_authorize,
        )))
        .route(
            "/{user_id}",
            get(get_user_by_id)
                .delete(delete_user)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .patch(update_user)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::authorize,
                )),
        )
        .route("/", post(register_user))
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

fn log_activity(state: &Arc<AppState>, mut log: ActivityLog, headers: &HeaderMap) {
    if let Some(ua) = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
    {
        log.set_user_agent(ua.to_string());
    }
    if let Some(ip) = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
    {
        log.set_ip_address(ip.to_string());
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Activity log channel full or closed, dropped log: {}", e);
    }
}

/// An Admin creates a user a assigns them a role type.
#[utoipa::path(
    post,
    path = "/users/admin",
    request_body = AdminCreateUserDto,
    responses(
        (status = 200, description = "User created successfully", body = UserDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Users"
)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Json(payload): Json<AdminCreateUserDto>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let response = services::create_user(state.clone(), payload).await?;

    let mut log = ActivityLog::new(ActivityType::UserCreated, claims.user_id);
    log.set_target_id(response.data.id);
    log.set_target_type("User".to_string());
    if let Ok(details) = serde_json::to_value(&response.data) {
        log.set_details(details);
    }
    log_activity(&state, log, &headers);

    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = "/users/{user_id}/role",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    request_body = AssignRoleDto,
    responses(
        (status = 200, description = "Role assigned to user successfully", body = UserDto),
        (status = 404, description = "User not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Users"
)]
pub async fn assign_role_to_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<AssignRoleDto>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let response =
        services::assign_role_to_user(state.clone(), user_id, payload.role.clone()).await?;

    let mut log = ActivityLog::new(ActivityType::UserUpdated, claims.user_id);
    log.set_target_id(user_id);
    log.set_target_type("User".to_string());
    log.set_details(serde_json::json!({
        "action": "assign_role",
        "role": payload.role,
    }));
    log_activity(&state, log, &headers);

    let response = Message::context("Role assigned to user successfully.", response);
    Ok(Json(response))
}

/// Register as a user, by default has a User Role. User registers their account.
#[utoipa::path(
    post,
    path = "/users/register",
    request_body = CreateUserDto,
    responses(
        (status = 200, description = "User registered successfully", body = UserDto)
    ),
    tag = "Users"
)]
pub async fn register_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<CreateUserDto>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let response = services::register_user(state.clone(), payload).await?;

    let mut log = ActivityLog::new(ActivityType::AccountCreated, response.data.id);
    log.set_target_id(response.data.id);
    log.set_target_type("User".to_string());
    if let Ok(details) = serde_json::to_value(&response.data) {
        log.set_details(details);
    }
    log_activity(&state, log, &headers);

    Ok(Json(response))
}

/// Get all users with search by username, email and names and filter by roles and is_active.
/// If pagination is empty, gets all users
#[utoipa::path(
    get,
    path = "/users",
    params(
        GetUsersQuery
    ),
    responses(
        (status = 200, description = "Users retrieved successfully", body = Vec<UserDto>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Users"
)]
pub async fn get_users(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetUsersQuery>,
) -> Result<Json<Message<Vec<UserDto>>>, ApiError> {
    let users = services::get_users(state, query).await?;
    let response = Message::context("Users retrieved successfully.", users);
    Ok(Json(response))
}

/// Get a user by ID
#[utoipa::path(
    get,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User retrieved successfully", body = UserDto),
        (status = 404, description = "User not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Users"
)]
pub async fn get_user_by_id(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let user = services::get_user_by_id(state, user_id).await?;
    let response = Message::context("User retrieved successfully.", user);
    Ok(Json(response))
}

/// Update a user
#[utoipa::path(
    patch,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    request_body = UpdateUserDto,
    responses(
        (status = 200, description = "User updated successfully", body = UserDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "User not found")
    ),
    tag = "Users"
)]
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserDto>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    if claims.role != RoleType::Admin && claims.role != RoleType::Root && claims.user_id != user_id
    {
        return Err(ApiError::PermissionDenied);
    }
    let user = services::update_user(state.clone(), user_id, payload.clone()).await?;

    let mut log = ActivityLog::new(ActivityType::UserUpdated, claims.user_id);
    log.set_target_id(user_id);
    log.set_target_type("User".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    log_activity(&state, log, &headers);

    let response = Message::context("User updated successfully.", user);
    Ok(Json(response))
}

/// Activate a user by admin
#[utoipa::path(
    patch,
    path = "/users/{user_id}/activate",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User activated successfully", body = UserDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "User not found")
    ),
    tag = "Users"
)]
pub async fn activate_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let user = services::activate_user(state.clone(), user_id).await?;

    let mut log = ActivityLog::new(ActivityType::UserActivation, claims.user_id);
    log.set_target_id(user_id);
    log.set_target_type("User".to_string());
    log_activity(&state, log, &headers);

    let response = Message::context("User activated successfully.", user);
    Ok(Json(response))
}

/// Deactivate a user by admin
#[utoipa::path(
    patch,
    path = "/users/{user_id}/deactivate",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User deactivated successfully", body = UserDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "User not found")
    ),
    tag = "Users"
)]
pub async fn deactivate_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let user = services::deactivate_user(state.clone(), user_id).await?;

    let mut log = ActivityLog::new(ActivityType::UserDeactivation, claims.user_id);
    log.set_target_id(user_id);
    log.set_target_type("User".to_string());
    log_activity(&state, log, &headers);

    let response = Message::context("User deactivated successfully.", user);
    Ok(Json(response))
}

/// Delete a user by admin
#[utoipa::path(
    delete,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User deleted successfully", body = UserDto),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "User not found")
    ),
    tag = "Users"
)]
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Message<UserDto>>, ApiError> {
    let user = services::delete_user(state.clone(), user_id).await?;

    let mut log = ActivityLog::new(ActivityType::UserDeleted, claims.user_id);
    log.set_target_id(user_id);
    log.set_target_type("User".to_string());
    log_activity(&state, log, &headers);

    let response = Message::context("User deleted successfully.", user);
    Ok(Json(response))
}
