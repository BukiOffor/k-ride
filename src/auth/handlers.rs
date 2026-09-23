use super::*;
use crate::Message;
use crate::auth::dto::{LoginPayload, LoginResponseDto};
use crate::logs::{ActivityLog, ActivityType};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router, extract::State};
use axum_extra::extract::CookieJar;
use std::sync::Arc;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/auth", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
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

/// User login endpoint
///
/// Authenticates a user with email/username and password.
/// Sets HTTP-only cookies (`access_token` and `refresh_token`) and returns the session token and user profile.
#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = LoginPayload,
    responses(
        (status = 200, description = "Login successful", body = Message<LoginResponseDto>),
        (status = 401, description = "Invalid credentials"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Auth"
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(payload): Json<LoginPayload>,
) -> Result<(CookieJar, Json<Message<LoginResponseDto>>), ModuleError> {
    let (jar, response) = service::login(jar, state.pool.clone(), payload).await?;

    let mut log = ActivityLog::new(ActivityType::UserLogin, response.user.id);
    log.set_target_id(response.user.id);
    log.set_target_type("User".to_string());
    log_activity(&state, log, &headers);

    let message = Message::context("Login successful", response);
    Ok((jar, Json(message)))
}

/// User logout endpoint
///
/// Clears authentication cookies.
#[utoipa::path(
    post,
    path = "/auth/logout",
    responses(
        (status = 200, description = "Logout successful", body = Message<String>)
    ),
    tag = "Auth"
)]
pub async fn logout(jar: CookieJar) -> (CookieJar, Json<Message<String>>) {
    let jar = service::logout(jar);
    let message = Message::context("Logout successful", "Session ended".to_string());
    (jar, Json(message))
}
