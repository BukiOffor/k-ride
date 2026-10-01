use super::*;
use crate::Message;
use crate::auth::jwt::Claims;
use crate::bus::models::ride_request::RideRequest;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/ride-requests", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", post(request_ride).get(get_ride_requests))
        .route(
            "/{ride_request_id}",
            delete(delete_ride_request)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_ride_request_by_id),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Request a ride or increment count on an existing request
///
/// Submits a ride request for a specific route and departure date. If a request already exists for that date and route, the passenger count is incremented.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
#[utoipa::path(
    post,
    path = "/ride-requests",
    request_body = CreateRideRequestDto,
    responses(
        (status = 200, description = "Ride request created or updated successfully", body = Message<RideRequest>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Route not found")
    ),
    tag = "Ride Requests"
)]
pub async fn request_ride(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateRideRequestDto>,
) -> Result<Json<Message<RideRequest>>, ApiError> {
    let request =
        services::ride_request::create_or_increment_ride_request(state, payload, claims.user_id)
            .await?;
    let response = Message::context("Ride request submitted successfully.", request);
    Ok(Json(response))
}

/// Search and filter ride requests
///
/// Retrieves ride requests with filters for route, departure date, and date range.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
#[utoipa::path(
    get,
    path = "/ride-requests",
    params(GetRideRequestsQuery),
    responses(
        (status = 200, description = "Ride requests retrieved successfully", body = Message<Vec<RideRequest>>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Ride Requests"
)]
pub async fn get_ride_requests(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetRideRequestsQuery>,
) -> Result<Json<Message<Vec<RideRequest>>>, ApiError> {
    let requests = services::ride_request::get_ride_requests(state, query).await?;
    let response = Message::context("Ride requests retrieved successfully.", requests);
    Ok(Json(response))
}

/// Get ride request details by ID
///
/// Retrieves the details of a specific ride request by its unique UUID.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
#[utoipa::path(
    get,
    path = "/ride-requests/{ride_request_id}",
    params(("ride_request_id" = Uuid, Path, description = "UUID of the ride request")),
    responses(
        (status = 200, description = "Ride request retrieved successfully", body = Message<RideRequest>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Ride request not found")
    ),
    tag = "Ride Requests"
)]
pub async fn get_ride_request_by_id(
    State(state): State<Arc<AppState>>,
    Path(ride_request_id): Path<Uuid>,
) -> Result<Json<Message<RideRequest>>, ApiError> {
    let request = services::ride_request::get_ride_request_by_id(state, ride_request_id).await?;
    let response = Message::context("Ride request retrieved successfully.", request);
    Ok(Json(response))
}

/// Delete a ride request (Admin only)
///
/// Removes a ride request entry.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    delete,
    path = "/ride-requests/{ride_request_id}",
    params(("ride_request_id" = Uuid, Path, description = "UUID of the ride request to delete")),
    responses(
        (status = 200, description = "Ride request deleted successfully", body = Message<RideRequest>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Ride request not found")
    ),
    tag = "Ride Requests"
)]
pub async fn delete_ride_request(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(ride_request_id): Path<Uuid>,
) -> Result<Json<Message<RideRequest>>, ApiError> {
    let request =
        services::ride_request::delete_ride_request(state, ride_request_id, claims.user_id).await?;
    let response = Message::context("Ride request deleted successfully.", request);
    Ok(Json(response))
}
