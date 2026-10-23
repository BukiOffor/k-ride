//! HTTP routing and request/response adapters for paid rides administration.
use crate::admin::{dto, dto::*, errors::AdminError, services};
use crate::{AppState, auth::jwt::Claims};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;
pub mod account;
pub mod applications;
pub mod people;
pub mod quality;
pub mod reports;
pub mod rides;
pub mod safety;
pub mod scheduled;

/// Mount the admin resource router under `/admin` for the API router.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new().nest("/admin", module_routes(state))
}
/// Register admin endpoints and apply the shared role gate before dispatching requests.
pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/scheduled-rides", get(scheduled::list))
        .route("/scheduled-rides/{id}/assign", post(scheduled::assign))
        .route("/overview", get(reports::overview))
        .route("/reports", get(reports::reports))
        .route("/reports/export", get(reports::export))
        .route("/rides", get(rides::rides))
        .route("/rides/{id}", get(rides::ride))
        .route("/rides/{id}/match", post(rides::match_driver))
        .route("/rides/{id}/assign", post(rides::match_driver))
        .route("/sos", get(safety::sos))
        .route("/sos/{id}/acknowledge", post(safety::acknowledge))
        .route("/sos/{id}/resolve", post(safety::resolve))
        .route(
            "/incidents",
            get(safety::incidents).post(safety::create_incident),
        )
        .route(
            "/incidents/{id}",
            get(safety::incident).patch(safety::update_incident),
        )
        .route("/incidents/{id}/notes", post(safety::add_note))
        .route("/incidents/{id}/calls", post(safety::log_call))
        .route("/drivers", get(people::drivers))
        .route("/drivers/live", get(people::live_drivers))
        .route("/drivers/{id}", get(people::driver))
        .route("/drivers/{id}/rides", get(people::driver_rides))
        .route(
            "/drivers/{id}/flag",
            post(people::flag).delete(people::unflag),
        )
        .route("/drivers/{id}/deactivate", post(people::deactivate))
        .route(
            "/drivers/{id}/inspection",
            post(applications::driver_inspection),
        )
        .route("/passengers", get(people::passengers))
        .route("/passengers/{id}", get(people::passenger))
        .route("/passengers/{id}/rides", get(people::passenger_rides))
        .route(
            "/passengers/{id}/restriction",
            post(people::restrict).delete(people::unrestrict),
        )
        .route(
            "/passengers/{id}/membership-check",
            post(people::membership_check),
        )
        .route("/quality-flags", get(quality::quality))
        .route("/quality-flags/{id}/clear", post(quality::clear_flag))
        .route("/driver-applications", get(applications::list))
        .route("/driver-applications/{id}", get(applications::detail))
        .route(
            "/driver-applications/{id}/documents/{doc}",
            patch(applications::document),
        )
        .route(
            "/driver-applications/{id}/comments",
            post(applications::comment),
        )
        .route(
            "/driver-applications/{id}/inspection",
            post(applications::inspection),
        )
        .route(
            "/driver-applications/{id}/decision",
            post(applications::decision),
        )
        .route_layer(axum::middleware::from_fn(admin_gate))
        .with_state(state)
}
/// Enforce Admin/Root access before invoking an admin HTTP handler.
async fn admin_gate(
    c: Claims,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, AdminError> {
    services::require_admin(&c)?;
    Ok(next.run(req).await)
}
