use super::*;
use crate::Message;
use crate::auth::jwt::Claims;
use crate::bus::models::trips::Trip;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/trips", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            post(create_trip)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_trips),
        )
        .route(
            "/{trip_id}",
            patch(update_trip)
                .delete(delete_trip)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_trip_by_id),
        )
        .route(
            "/{trip_id}/start",
            patch(start_trip).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::coordinator_authorize,
            )),
        )
        .route(
            "/{trip_id}/complete",
            patch(complete_trip).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::coordinator_authorize,
            )),
        )
        .route(
            "/{trip_id}/cancel",
            patch(cancel_trip).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Schedule a new trip on a route (Admin only)
///
/// Creates a scheduled trip referencing a persistent route definition, with departure/arrival times, optional bus and driver assignment, and ride type.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/trips",
    request_body = CreateTripDto,
    responses(
        (status = 200, description = "Trip created successfully", body = Message<Trip>),
        (status = 400, description = "Bad request - Validation failure"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Route not found")
    ),
    tag = "Trips"
)]
pub async fn create_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateTripDto>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::create_trip(state, payload, claims.user_id).await?;
    let response = Message::context("Trip created successfully.", trip);
    Ok(Json(response))
}

/// Search and filter scheduled trips
///
/// Retrieves trips with filters for route, bus, driver, status, ride type, and date range.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/trips",
    params(GetTripsQuery),
    responses(
        (status = 200, description = "Trips retrieved successfully", body = Message<Vec<Trip>>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Trips"
)]
pub async fn get_trips(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetTripsQuery>,
) -> Result<Json<Message<Vec<Trip>>>, ApiError> {
    let trips = services::trips::get_trips(state, query).await?;
    let response = Message::context("Trips retrieved successfully.", trips);
    Ok(Json(response))
}

/// Get trip details by ID
///
/// Retrieves full trip specifications including route reference, bus/driver assignment, timings, and status.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/trips/{trip_id}",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip")),
    responses(
        (status = 200, description = "Trip retrieved successfully", body = Message<Trip>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn get_trip_by_id(
    State(state): State<Arc<AppState>>,
    Path(trip_id): Path<Uuid>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::get_trip_by_id(state, trip_id).await?;
    let response = Message::context("Trip retrieved successfully.", trip);
    Ok(Json(response))
}

/// Update trip details (Admin only)
///
/// Updates bus/driver assignment, timings, ride type, or status for a scheduled trip.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/trips/{trip_id}",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip to update")),
    request_body = UpdateTripDto,
    responses(
        (status = 200, description = "Trip updated successfully", body = Message<Trip>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn update_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(trip_id): Path<Uuid>,
    Json(payload): Json<UpdateTripDto>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::update_trip(state, trip_id, payload, claims.user_id).await?;
    let response = Message::context("Trip updated successfully.", trip);
    Ok(Json(response))
}

/// Delete a scheduled trip (Admin only)
///
/// Permanently removes a trip from the system.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    delete,
    path = "/trips/{trip_id}",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip to delete")),
    responses(
        (status = 200, description = "Trip deleted successfully", body = Message<Trip>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn delete_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(trip_id): Path<Uuid>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::delete_trip(state, trip_id, claims.user_id).await?;
    let response = Message::context("Trip deleted successfully.", trip);
    Ok(Json(response))
}

/// Start a trip (Coordinator/Admin)
///
/// Marks a trip as in-progress and records the actual departure time.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin`, `Root`, or `Coordinator` role.
#[utoipa::path(
    patch,
    path = "/trips/{trip_id}/start",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip to start")),
    responses(
        (status = 200, description = "Trip started successfully", body = Message<Trip>),
        (status = 400, description = "Bad request - Trip is not in NotStarted status"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn start_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(trip_id): Path<Uuid>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::start_trip(state, trip_id, claims.user_id).await?;
    let response = Message::context("Trip started successfully.", trip);
    Ok(Json(response))
}

/// Complete a trip (Coordinator/Admin)
///
/// Marks a trip as completed and records the actual arrival time.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin`, `Root`, or `Coordinator` role.
#[utoipa::path(
    patch,
    path = "/trips/{trip_id}/complete",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip to complete")),
    responses(
        (status = 200, description = "Trip completed successfully", body = Message<Trip>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn complete_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(trip_id): Path<Uuid>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::complete_trip(state, trip_id, claims.user_id).await?;
    let response = Message::context("Trip completed successfully.", trip);
    Ok(Json(response))
}

/// Cancel a trip (Admin only)
///
/// Cancels a scheduled trip. Completed trips cannot be cancelled.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/trips/{trip_id}/cancel",
    params(("trip_id" = Uuid, Path, description = "UUID of the trip to cancel")),
    responses(
        (status = 200, description = "Trip cancelled successfully", body = Message<Trip>),
        (status = 400, description = "Bad request - Cannot cancel a completed trip"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Trip not found")
    ),
    tag = "Trips"
)]
pub async fn cancel_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(trip_id): Path<Uuid>,
) -> Result<Json<Message<Trip>>, ApiError> {
    let trip = services::trips::cancel_trip(state, trip_id, claims.user_id).await?;
    let response = Message::context("Trip cancelled successfully.", trip);
    Ok(Json(response))
}
