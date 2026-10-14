//! HTTP Handlers for Driver endpoints (availability, offers, trip operations, location).

use super::*;
use crate::auth::jwt::Claims;

use axum::middleware;
use std::sync::Arc;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    Router::new().nest("/driver", routes)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/availability", get(get_driver_availability).put(set_driver_availability))
        .route("/rides/current", get(get_driver_current_ride))
        .route("/rides/history", get(get_driver_history))
        .route("/offers/{offer_id}/accept", post(accept_offer))
        .route("/offers/{offer_id}/decline", post(decline_offer))
        .route("/rides/{ride_id}/arrived", post(driver_arrived))
        .route("/rides/{ride_id}/start", post(driver_start_ride))
        .route("/rides/{ride_id}/complete", post(driver_complete_ride))
        .route("/rides/{ride_id}/cancel", post(driver_cancel_ride))
        .route("/location", post(update_driver_location))
        .layer(tower::ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

#[utoipa::path(
    get,
    path = "/driver/availability",
    responses(
        (status = 200, description = "Driver live availability", body = AvailabilityEnvelope),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn get_driver_availability(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<AvailabilityEnvelope>, ApiError> {
    let res = services::drivers::get_driver_availability(state, claims).await?;
    Ok(Json(res))
}

#[utoipa::path(
    put,
    path = "/driver/availability",
    request_body = SetAvailabilityDto,
    responses(
        (status = 200, description = "Availability updated", body = AvailabilityEnvelope),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn set_driver_availability(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<SetAvailabilityDto>,
) -> Result<Json<AvailabilityEnvelope>, ApiError> {
    let res = services::drivers::set_driver_availability(state, claims, payload).await?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/driver/rides/current",
    responses(
        (status = 200, description = "Driver current active ride", body = RideEnvelope),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn get_driver_current_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<RideEnvelope>, ApiError> {
    let res = services::drivers::get_driver_current_ride(state, claims).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/driver/offers/{offer_id}/accept",
    params(("offer_id" = Uuid, Path, description = "ID of the offer")),
    responses(
        (status = 200, description = "Offer accepted, ride assigned", body = RideProjection),
        (status = 400, description = "Already taken or expired"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn accept_offer(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(offer_id): Path<Uuid>,
) -> Result<Json<RideProjection>, ApiError> {
    let res = services::drivers::accept_ride_offer(state, claims, offer_id).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/driver/offers/{offer_id}/decline",
    params(("offer_id" = Uuid, Path, description = "ID of the offer")),
    responses(
        (status = 204, description = "Offer declined"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn decline_offer(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(offer_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, ApiError> {
    services::drivers::decline_ride_offer(state, claims, offer_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/driver/rides/{ride_id}/arrived",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    responses(
        (status = 200, description = "Driver marked arrived at pickup", body = RideProjection),
        (status = 400, description = "Invalid transition"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn driver_arrived(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
) -> Result<Json<RideProjection>, ApiError> {
    let res = services::drivers::driver_arrived(state, claims, ride_id).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/driver/rides/{ride_id}/start",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = StartRideDto,
    responses(
        (status = 200, description = "Ride started (PIN verified if provided)", body = RideProjection),
        (status = 400, description = "Invalid PIN or state"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn driver_start_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<StartRideDto>,
) -> Result<Json<RideProjection>, ApiError> {
    let res = services::drivers::start_ride(state, claims, ride_id, payload).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/driver/rides/{ride_id}/complete",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    responses(
        (status = 200, description = "Ride completed and final fare calculated", body = RideProjection),
        (status = 400, description = "Invalid transition"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn driver_complete_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
) -> Result<Json<RideProjection>, ApiError> {
    let res = services::drivers::complete_ride(state, claims, ride_id).await?;
    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/driver/rides/{ride_id}/cancel",
    params(("ride_id" = Uuid, Path, description = "ID of the ride")),
    request_body = CancelRideDto,
    responses(
        (status = 200, description = "Driver cancelled, ride reset to searching", body = RideProjection),
        (status = 400, description = "Cannot cancel after trip start"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn driver_cancel_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_id): Path<Uuid>,
    Json(payload): Json<CancelRideDto>,
) -> Result<Json<RideProjection>, ApiError> {
    let res = services::drivers::driver_cancel_ride(state, claims, ride_id, payload).await?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/driver/rides/history",
    responses(
        (status = 200, description = "Driver trip history", body = RideHistoryPage),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn get_driver_history(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<RideHistoryPage>, ApiError> {
    let history = services::drivers::get_driver_history(state, claims, query).await?;
    Ok(Json(history))
}

#[utoipa::path(
    post,
    path = "/driver/location",
    request_body = LiveLocation,
    responses(
        (status = 204, description = "Location recorded"),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Driver"
)]
pub async fn update_driver_location(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<LiveLocation>,
) -> Result<axum::http::StatusCode, ApiError> {
    services::drivers::update_driver_location_rest(state, claims, payload).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
