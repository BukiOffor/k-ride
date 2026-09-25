use super::*;
use crate::Message;
use crate::auth::jwt::Claims;
use crate::bus::models::routes::Route;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/routes", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            post(create_route)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_routes),
        )
        .route(
            "/{route_id}",
            patch(update_route)
                .delete(delete_route)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_route_by_id),
        )
        .route(
            "/{route_id}/complete",
            patch(complete_route).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::coordinator_authorize,
            )),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Create a transit route (Admin only)
///
/// Sets up a scheduled route connecting an origin and destination point, complete with departure/arrival times, fare pricing, and ride category.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Round-Trip Auto-Generation
/// - If `is_round_trip` is set to `true`, the system automatically provisions **both** the outbound trip and an inverted return route:
///   - Origin and Destination points are swapped.
///   - Intermediate waypoints are reversed.
///   - Return departure and arrival times are calculated by applying the optional `return_delay_hours` offset (or default duration).
///   - Links both legs using a common round-trip reference.
///
/// ### Returns
/// A list containing the created route(s) (`[outbound]` or `[outbound, return]`).
#[utoipa::path(
    post,
    path = "/routes",
    request_body = CreateRouteDto,
    responses(
        (status = 200, description = "Route(s) created successfully", body = Message<Vec<Route>>),
        (status = 400, description = "Bad request - Validation failure or invalid origin/destination IDs"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
    ),
    tag = "Routes"
)]
pub async fn create_route(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateRouteDto>,
) -> Result<Json<Message<Vec<Route>>>, ApiError> {
    let routes = services::routes::create_route(state, payload, claims.user_id).await?;
    let message = if routes.len() > 1 {
        "Routes created successfully (round trip)."
    } else {
        "Route created successfully."
    };
    let response = Message::context(message, routes);
    Ok(Json(response))
}

/// Search and filter scheduled routes
///
/// Retrieves scheduled transit routes with filters for origin, destination, ride type, and departure window.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, `Driver`, etc.).
///
/// ### Query Parameters
/// - `origin_id`: Filter by starting bus stop / terminal UUID.
/// - `destination_id`: Filter by final destination terminal UUID.
/// - `ride_type`: Filter by category (`Intercity`, `Intracity`, `Interstate`, `Campus`).
/// - `page` & `limit`: Pagination controls.
#[utoipa::path(
    get,
    path = "/routes",
    params(
        GetRoutesQuery
    ),
    responses(
        (status = 200, description = "Routes retrieved successfully", body = Message<Vec<Route>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token")
    ),
    tag = "Routes"
)]
pub async fn get_routes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetRoutesQuery>,
) -> Result<Json<Message<Vec<Route>>>, ApiError> {
    let routes = services::routes::get_routes(state, query).await?;
    let response = Message::context("Routes retrieved successfully.", routes);
    Ok(Json(response))
}

/// Get route details by ID
///
/// Retrieves full route specifications including origin, destination, intermediate stop waypoints, departure time, fare, and completion state.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, `Driver`, etc.).
#[utoipa::path(
    get,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Unique UUID of the route to retrieve")
    ),
    responses(
        (status = 200, description = "Route retrieved successfully", body = Message<Route>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 404, description = "Route not found")
    ),
    tag = "Routes"
)]
pub async fn get_route_by_id(
    State(state): State<Arc<AppState>>,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Message<Route>>, ApiError> {
    let route = services::routes::get_route_by_id(state, route_id).await?;
    let response = Message::context("Route retrieved successfully.", route);
    Ok(Json(response))
}

/// Update route details (Admin only)
///
/// Updates timetable, fare, waypoints, or active status for an existing transit route.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Unique UUID of the route to update")
    ),
    request_body = UpdateRouteDto,
    responses(
        (status = 200, description = "Route updated successfully", body = Message<Route>),
        (status = 400, description = "Bad request - Invalid payload data"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Route not found")
    ),
    tag = "Routes"
)]
pub async fn update_route(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(route_id): Path<Uuid>,
    Json(payload): Json<UpdateRouteDto>,
) -> Result<Json<Message<Route>>, ApiError> {
    let route = services::routes::update_route(state, route_id, payload, claims.user_id).await?;
    let response = Message::context("Route updated successfully.", route);
    Ok(Json(response))
}

/// Delete a route (Admin only)
///
/// Permanently deletes a scheduled route from the system.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Constraints
/// - Routes with existing passenger bookings or actively assigned buses cannot be deleted.
#[utoipa::path(
    delete,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Unique UUID of the route to delete")
    ),
    responses(
        (status = 200, description = "Route deleted successfully", body = Message<Route>),
        (status = 400, description = "Bad request - Route has active bookings or buses assigned"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Route not found")
    ),
    tag = "Routes"
)]
pub async fn delete_route(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Message<Route>>, ApiError> {
    let route = services::routes::delete_route(state, route_id, claims.user_id).await?;
    let response = Message::context("Route deleted successfully.", route);
    Ok(Json(response))
}

/// Mark a route as completed (Admin only)
///
/// Concludes a route trip after the bus has safely reached the final terminal. Sets `is_completed = true` and records the `completed_at` timestamp.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/routes/{route_id}/complete",
    params(
        ("route_id" = Uuid, Path, description = "Unique UUID of the route to complete")
    ),
    responses(
        (status = 200, description = "Route marked as completed successfully", body = Message<Route>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Route not found")
    ),
    tag = "Routes"
)]
pub async fn complete_route(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(route_id): Path<Uuid>,
) -> Result<Json<Message<Route>>, ApiError> {
    let route = services::routes::complete_route(state, route_id, claims.user_id).await?;
    let response = Message::context("Route marked as completed successfully.", route);
    Ok(Json(response))
}
