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
            delete(delete_user)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .patch(update_user)
                .get(get_user_by_id)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::authorize,
                )),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .route("/", post(register_user))
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

/// Create a user and assign a role (Admin only)
///
/// Provisions a new user account with an explicitly assigned role (`User`, `Admin`, `Driver`, `Inspector`, or `Root`).
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Validates unique email and username constraints.
/// - Hashes the account password securely with bcrypt before persisting.
/// - Records an `ActivityType::UserCreated` audit log entry with actor and target IDs.
/// - Returns the created user's sanitized profile (`UserDto`).
#[utoipa::path(
    post,
    path = "/users/admin",
    request_body = AdminCreateUserDto,
    responses(
        (status = 200, description = "User created successfully", body = UserDto),
        (status = 400, description = "Invalid request payload or duplicate email/username"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
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

/// Assign or change a user's role (Admin only)
///
/// Updates the authorization role for an existing user account.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Available roles: `User`, `Admin`, `Driver`, `Inspector`, `Root`.
/// - Records an `ActivityType::UserUpdated` audit log entry detailing the transition.
/// - Returns the updated user profile reflecting the newly assigned role.
#[utoipa::path(
    patch,
    path = "/users/{user_id}/role",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user whose role is being changed")
    ),
    request_body = AssignRoleDto,
    responses(
        (status = 200, description = "Role assigned successfully", body = UserDto),
        (status = 400, description = "Invalid role value provided"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "User not found")
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

/// Self-register a new user account (Public)
///
/// Allows new public customers to register for an account on the K-Ride platform.
///
/// ### Access Control
/// - **Public:** No authentication token required.
///
/// ### Behavior
/// - Automatically assigns the standard `User` role.
/// - Sets account status to active (`is_active = true`).
/// - Hashes the password with bcrypt before storing.
/// - Records an `ActivityType::AccountCreated` audit log entry.
/// - Returns the newly registered user's profile (`UserDto`).
#[utoipa::path(
    post,
    path = "/users",
    request_body = CreateUserDto,
    responses(
        (status = 200, description = "User registered successfully", body = UserDto),
        (status = 400, description = "Validation error (e.g. invalid email format, short password, or duplicate email/username)")
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

/// List and search users with filters (Admin only)
///
/// Retrieves a list of users supporting multi-field search, role filtering, active status filter, and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Query Parameters
/// - `search`: Case-insensitive partial matching against `username`, `email`, `first_name`, and `last_name`.
/// - `roles`: Filter users by specific role (`User`, `Admin`, `Driver`, etc.).
/// - `is_active`: Filter by active (`true`) or deactivated (`false`) status.
/// - `page` & `limit`: Optional pagination window (if omitted, all matched records are returned).
#[utoipa::path(
    get,
    path = "/users",
    params(
        GetUsersQuery
    ),
    responses(
        (status = 200, description = "Users retrieved successfully", body = Vec<UserDto>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
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

/// Get user profile by ID
///
/// Fetches detailed profile information for a single user given their UUID.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role, or an authenticated user retrieving their own record.
///
/// ### Response
/// Returns the user's public profile data (`UserDto`), including contact info, role, and active status.
#[utoipa::path(
    get,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user to retrieve")
    ),
    responses(
        (status = 200, description = "User retrieved successfully", body = UserDto),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Insufficient permissions"),
        (status = 404, description = "User not found")
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

/// Update user profile details
///
/// Modifies profile information (e.g. name, email, phone number, avatar URL) for an existing user.
///
/// ### Access Control
/// - **Self-update:** Any authenticated user may update their own profile.
/// - **Admin-update:** Users with `Admin` or `Root` roles may update any user profile.
///
/// ### Behavior
/// - Only provided non-null fields in the request body are modified.
/// - Records an `ActivityType::UserUpdated` audit log entry detailing modified fields.
#[utoipa::path(
    patch,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user to update")
    ),
    request_body = UpdateUserDto,
    responses(
        (status = 200, description = "User updated successfully", body = UserDto),
        (status = 400, description = "Bad request - Invalid input payload"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - You can only edit your own profile unless Admin/Root"),
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

/// Activate a suspended user account (Admin only)
///
/// Re-enables account access for a previously deactivated user by setting `is_active = true`.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Once activated, the user can successfully log in and book rides again.
/// - Records an `ActivityType::UserActivation` audit log.
#[utoipa::path(
    patch,
    path = "/users/{user_id}/activate",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user account to activate")
    ),
    responses(
        (status = 200, description = "User activated successfully", body = UserDto),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Deactivate a user account (Admin only)
///
/// Suspends a user account by setting `is_active = false`.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Suspended accounts are immediately blocked from authentication and API operations.
/// - Records an `ActivityType::UserDeactivation` audit log.
#[utoipa::path(
    patch,
    path = "/users/{user_id}/deactivate",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user account to deactivate")
    ),
    responses(
        (status = 200, description = "User deactivated successfully", body = UserDto),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Permanently delete a user account (Admin only)
///
/// Hard deletes a user record from the database.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Removes the user record and dependent cascades.
/// - Records an `ActivityType::UserDeleted` audit log.
/// - Returns the removed user's record data for confirmation.
#[utoipa::path(
    delete,
    path = "/users/{user_id}",
    params(
        ("user_id" = Uuid, Path, description = "Unique UUID of the user account to delete")
    ),
    responses(
        (status = 200, description = "User deleted successfully", body = UserDto),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
