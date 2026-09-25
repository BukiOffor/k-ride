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
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Create a transit route (Admin only)
///
/// Defines a persistent geographic route connecting an origin and destination point, with fare pricing and intermediate stops.
/// Routes are reusable path definitions. To schedule an actual departure, create a Trip referencing this route.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Returns
/// The created route.
#[utoipa::path(
    post,
    path = "/routes",
    request_body = CreateRouteDto,
    responses(
        (status = 200, description = "Route created successfully", body = Message<Route>),
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
) -> Result<Json<Message<Route>>, ApiError> {
    let route = services::routes::create_route(state, payload, claims.user_id).await?;
    let response = Message::context("Route created successfully.", route);
    Ok(Json(response))
}

/// Search and filter routes
///
/// Retrieves transit routes with filters for origin, destination, and text search.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
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
/// Retrieves full route specifications including origin, destination, intermediate stop waypoints, and fare.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
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
/// Updates fare, waypoints, or descriptive information for an existing transit route.
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
/// Permanently deletes a route definition from the system.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Constraints
/// - Routes with existing trips or active bookings cannot be deleted.
#[utoipa::path(
    delete,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Unique UUID of the route to delete")
    ),
    responses(
        (status = 200, description = "Route deleted successfully", body = Message<Route>),
        (status = 400, description = "Bad request - Route has active trips or bookings"),
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
