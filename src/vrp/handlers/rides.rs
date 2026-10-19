//! HTTP Handlers for Paid Rides API (Passenger endpoints).

use super::*;
use crate::auth::jwt::Claims;
use axum::http::HeaderMap;
use axum::middleware;
use std::sync::Arc;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    Router::new().nest("/rides", routes)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/me", get(get_viewer))
        .route(
            "/me/driver-review",
            get(crate::admin::handlers::applications::own_review),
        )
        .route(
            "/me/notifications",
            get(crate::admin::handlers::account::notifications),
        )
        .route(
            "/me/trusted-contacts",
            get(crate::admin::handlers::account::contacts)
                .post(crate::admin::handlers::account::add_contact),
        )
        .route(
            "/me/trusted-contacts/{id}",
            axum::routing::delete(crate::admin::handlers::account::remove_contact),
        )
        .route("/estimate", post(estimate_fare))
        .route("/request", post(request_ride))
        .route("/current", get(get_current_ride))
        .route("/history", get(get_passenger_history))
        .route("/{ride_id}", get(get_ride_by_id))
        .route("/{ride_id}/cancel", post(cancel_ride))
        .route("/{ride_id}/retry", post(retry_ride))
        .route("/{ride_id}/rating", post(rate_ride))
        .route("/{ride_id}/sos", post(report_sos))
        .route("/{ride_id}/safety-reports", post(report_safety_issue))
        .route("/{ride_id}/share-link", post(share_link))
        .layer(
            tower::ServiceBuilder::new().layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::authorize,
            )),
        )
        .with_state(state)
}

// =========================================================================================
// Passenger Handlers
// =========================================================================================

#[utoipa::path(
    get,
    path = "/rides/me",
    responses(
        (status = 200, description = "Viewer profile", body = ViewerDto),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn get_viewer(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<ViewerDto>, ApiError> {
    let viewer = services::rides::get_viewer_profile(state, claims).await?;
    Ok(Json(viewer))
}

#[utoipa::path(
    post,
    path = "/rides/estimate",
    request_body = EstimateRequestDto,
    responses(
        (status = 200, description = "Dynamic fare estimates across ride options", body = FareEstimateDto),
        (status = 400, description = "Invalid coordinates or points"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn estimate_fare(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<EstimateRequestDto>,
) -> Result<Json<FareEstimateDto>, ApiError> {
    let estimate = services::rides::estimate_fare(state, claims, payload).await?;
    Ok(Json(estimate))
}

#[utoipa::path(
    post,
    path = "/rides/request",
    request_body = RideRequestDto,
    responses(
        (status = 200, description = "Ride requested and searching", body = RideProjection),
        (status = 400, description = "Estimate expired or invalid"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn request_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    headers: HeaderMap,
    Json(payload): Json<RideRequestDto>,
) -> Result<Json<RideProjection>, ApiError> {
    // Check idempotency key if provided
    if let Some(key) = headers.get("Idempotency-Key").and_then(|h| h.to_str().ok()) {
        let mut conn = state
            .pool
            .get()
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
        if let Some((_, cached_json)) =
            services::rides::check_idempotency(&mut conn, claims.user_id, "/rides/request", key)
                .await?
        {
            if let Ok(cached_ride) = serde_json::from_value::<RideProjection>(cached_json) {
                return Ok(Json(cached_ride));
            }
        }
    }

    let ride = services::rides::request_ride(state.clone(), claims.clone(), payload).await?;

    if let Some(key) = headers.get("Idempotency-Key").and_then(|h| h.to_str().ok()) {
        if let Ok(mut conn) = state.pool.get().await {
            if let Ok(val) = serde_json::to_value(&ride) {
                let _ = services::rides::store_idempotency(
                    &mut conn,
                    claims.user_id,
                    "/rides/request",
                    key,
                    200,
                    &val,
                )
                .await;
            }
        }
    }

    Ok(Json(ride))
}

#[utoipa::path(
    get,
    path = "/rides/current",
    responses(
        (status = 200, description = "Current active or recently completed ride", body = RideEnvelope),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn get_current_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<RideEnvelope>, ApiError> {
    let envelope = services::rides::get_current_ride(state, claims).await?;
    Ok(Json(envelope))
}

#[utoipa::path(
    get,
    path = "/rides/{ride_id}",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    responses(
        (status = 200, description = "Ride projection", body = RideProjection),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    ),
    tag = "Paid Rides"
)]
pub async fn get_ride_by_id(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
) -> Result<Json<RideProjection>, ApiError> {
    let ride = services::rides::get_ride_by_id(state, claims, ride_id).await?;
    Ok(Json(ride))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/cancel",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = CancelRideDto,
    responses(
        (status = 200, description = "Ride cancelled", body = RideProjection),
        (status = 400, description = "Cannot cancel ride in its current state"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn cancel_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<CancelRideDto>,
) -> Result<Json<RideProjection>, ApiError> {
    let ride = services::rides::cancel_ride_passenger(state, claims, ride_id, payload).await?;
    Ok(Json(ride))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/retry",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = RetryRideDto,
    responses(
        (status = 200, description = "Retrying driver search", body = RideProjection),
        (status = 400, description = "Ride not in NO_DRIVERS status"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn retry_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<RetryRideDto>,
) -> Result<Json<RideProjection>, ApiError> {
    let ride = services::rides::retry_ride_search(state, claims, ride_id, payload).await?;
    Ok(Json(ride))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/rating",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = RateRideDto,
    responses(
        (status = 200, description = "Rating submitted", body = RideProjection),
        (status = 400, description = "Invalid rating or already rated"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn rate_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<RateRideDto>,
) -> Result<Json<RideProjection>, ApiError> {
    let ride = services::rides::rate_ride(state, claims, ride_id, payload).await?;
    Ok(Json(ride))
}

#[utoipa::path(
    get,
    path = "/rides/history",
    responses(
        (status = 200, description = "Passenger ride history", body = RideHistoryPage),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn get_passenger_history(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<RideHistoryPage>, ApiError> {
    let history = services::rides::get_passenger_history(state, claims, query).await?;
    Ok(Json(history))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/sos",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = SosDto,
    responses(
        (status = 202, description = "Emergency SOS alert received", body = AcceptedDto),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn report_sos(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<SosDto>,
) -> Result<(axum::http::StatusCode, Json<AcceptedDto>), ApiError> {
    let res = services::rides::report_sos(state, claims, ride_id, payload).await?;
    Ok((axum::http::StatusCode::ACCEPTED, Json(res)))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/safety-reports",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = SafetyReportDto,
    responses(
        (status = 202, description = "Safety report filed", body = AcceptedDto),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn report_safety_issue(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<SafetyReportDto>,
) -> Result<(axum::http::StatusCode, Json<AcceptedDto>), ApiError> {
    let res = services::rides::report_safety_issue(state, claims, ride_id, payload).await?;
    Ok((axum::http::StatusCode::ACCEPTED, Json(res)))
}

#[utoipa::path(
    post,
    path = "/rides/{ride_id}/share-link",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    responses(
        (status = 200, description = "Live tracking share link", body = ShareLinkDto),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Paid Rides"
)]
pub async fn share_link(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
) -> Result<Json<ShareLinkDto>, ApiError> {
    let res = services::rides::generate_share_link(state, claims, ride_id).await?;
    Ok(Json(res))
}
