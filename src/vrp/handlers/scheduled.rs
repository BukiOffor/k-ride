//! HTTP adapters for passenger scheduled bookings and the driver request board.
use crate::auth::jwt::Claims;
use crate::vrp::dtos::scheduled::*;
use crate::vrp::services::scheduled;
use crate::{AppState, ModuleError};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Mount scheduled routes before the existing ride-ID routes; all handlers require JWT/cookie claims.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/rides/scheduled", get(list).post(create))
        .route("/rides/scheduled/{id}", get(detail).patch(reschedule))
        .route("/rides/scheduled/{id}/cancel", post(cancel))
        .route("/driver/scheduled-requests", get(board))
        .route("/driver/scheduled-requests/{id}/accept", post(accept))
        .route("/driver/scheduled-requests/{id}/cancel", post(release))
        .with_state(state)
}
/// Save the passenger's booking and locked quote; initial broadcast and emails are queued atomically.
#[utoipa::path(post,path="/rides/scheduled",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),request_body=ScheduleRideDto,responses((status=201,body=ScheduledRideDto),(status=400,description="Invalid booking"),(status=409,description="Scheduling conflict")))]
pub async fn create(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<ScheduleRideDto>,
) -> Result<(StatusCode, Json<Value>), ModuleError> {
    Ok((StatusCode::CREATED, Json(scheduled::create(s, c, p).await?)))
}
/// List only the authenticated passenger's scheduled bookings.
#[utoipa::path(get,path="/rides/scheduled",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(ScheduledQuery),responses((status=200,body=ScheduledRidePage)))]
pub async fn list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(q): Query<ScheduledQuery>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::list(s, c, q, false).await?))
}
/// Return authorized scheduled-booking detail without phone or document data.
#[utoipa::path(get,path="/rides/scheduled/{id}",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),responses((status=200,body=ScheduledRideDto),(status=404,description="Not found")))]
pub async fn detail(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::detail(s, c, id).await?))
}
/// Change pickup time and reopen the booking for fresh driver acceptance while retaining its fare.
#[utoipa::path(patch,path="/rides/scheduled/{id}",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),request_body=RescheduleRideDto,responses((status=200,body=ScheduledRideDto),(status=409,description="Cannot reschedule")))]
pub async fn reschedule(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<RescheduleRideDto>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::reschedule(s, c, id, p).await?))
}
/// Cancel the passenger's future booking; activated journeys use the existing live-ride cancellation route.
#[utoipa::path(post,path="/rides/scheduled/{id}/cancel",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),request_body=ScheduledCancellationDto,responses((status=200,body=ScheduledRideDto),(status=409,description="Booking closed or activated")))]
pub async fn cancel(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<ScheduledCancellationDto>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::cancel(s, c, id, p, false).await?))
}
/// List eligible open requests, or the authenticated driver's reservations when mine=true.
#[utoipa::path(get,path="/driver/scheduled-requests",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(ScheduledQuery),responses((status=200,body=ScheduledRidePage),(status=403,description="Driver required")))]
pub async fn board(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(q): Query<ScheduledQuery>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::list(s, c, q, true).await?))
}
/// Atomically accept a request from either its broadcast card or the scheduled-request board.
#[utoipa::path(post,path="/driver/scheduled-requests/{id}/accept",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),request_body=AcceptScheduledRideDto,responses((status=200,body=ScheduledRideDto),(status=409,description="Taken or overlapping reservation")))]
pub async fn accept(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<AcceptScheduledRideDto>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::accept(s, c, id, p.version).await?))
}
/// Release the assigned driver's reservation and queue a fresh broadcast for other drivers.
#[utoipa::path(post,path="/driver/scheduled-requests/{id}/cancel",tag="Scheduled Rides",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),request_body=ScheduledCancellationDto,responses((status=200,body=ScheduledRideDto),(status=409,description="Booking closed or activated")))]
pub async fn release(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<ScheduledCancellationDto>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(scheduled::cancel(s, c, id, p, true).await?))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(create, list, detail, reschedule, cancel, board, accept, release))]
pub struct ScheduledApiDoc;
