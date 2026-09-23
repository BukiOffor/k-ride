use super::*;

use crate::auth::jwt::Claims;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/points", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            post(create_points)
                .patch(update_points)
                .delete(delete_points)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_points),
        )
        .route("/{point_id}", get(get_point_by_id))
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// List bus stop / terminal points
///
/// Retrieves all transit points (stations, terminals, pick-up/drop-off stops) with optional search filtering and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, etc.).
///
/// ### Query Parameters
/// - `search`: Case-insensitive search on point `name`, `landmark`, or `description`.
/// - `page` & `limit`: Pagination controls (defaults to all points if omitted).
#[utoipa::path(
    get,
    path = "/points",
    params(
        GetPointsQuery
    ),
    responses(
        (status = 200, description = "Points retrieved successfully", body = Message<Vec<Point>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token")
    ),
    tag = "Points"
)]
pub async fn get_points(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetPointsQuery>,
) -> Result<Json<Message<Vec<Point>>>, ApiError> {
    let points = services::points::get_points(state, query).await?;
    let response = Message::context("Points retrieved successfully.", points);
    Ok(Json(response))
}

/// Create a bus stop / terminal point (Admin only)
///
/// Adds a new physical bus station, terminal, or roadside stop into the transit network.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Request Body
/// - `name`: Distinct human-readable identifier (e.g. "Lekki Phase 1 Terminal").
/// - `geo_location`: GPS coordinates (`latitude` and `longitude`).
/// - `landmark`: Nearby recognizable landmark for passengers.
/// - `description`: Additional directions or location instructions.
#[utoipa::path(
    post,
    path = "/points",
    request_body = CreatePointDto,
    responses(
        (status = 200, description = "Point created successfully", body = Message<Point>),
        (status = 400, description = "Bad request - Validation error in request body"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
    ),
    tag = "Points"
)]
pub async fn create_points(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreatePointDto>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point = services::points::create_point(state, payload, claims.user_id).await?;
    let response = Message::context("Point created successfully.", point);
    Ok(Json(response))
}

/// Update a bus stop / terminal point (Admin only)
///
/// Modifies details of an existing transit point.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Identifier Resolution
/// - The target Point ID can be specified either via the query parameter `?id=...` or directly inside the JSON request body `id`.
#[utoipa::path(
    patch,
    path = "/points",
    params(
        ("id" = Option<Uuid>, Query, description = "Unique UUID of the point to update (optional if provided in body)")
    ),
    request_body = UpdatePointDto,
    responses(
        (status = 200, description = "Point updated successfully", body = Message<Point>),
        (status = 400, description = "Bad request - Point ID missing or invalid coordinates"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Point not found")
    ),
    tag = "Points"
)]
pub async fn update_points(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Query(query): Query<PointIdQuery>,
    Json(payload): Json<UpdatePointDto>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point_id = query
        .id
        .or(payload.id)
        .ok_or_else(|| ApiError::BadRequest("Point ID is required"))?;
    let update_payload = UpdatePointPayload {
        name: payload.name.clone(),
        geo_location: payload.geo_location.clone(),
        landmark: payload.landmark.clone(),
        description: payload.description.clone(),
    };
    let point =
        services::points::update_point(state, point_id, update_payload, claims.user_id).await?;
    let response = Message::context("Point updated successfully.", point);
    Ok(Json(response))
}

/// Delete a bus stop / terminal point (Admin only)
///
/// Permanently removes a transit point from the system.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Constraints
/// - Points that are currently linked as an origin, destination, or waypoint in an existing route cannot be deleted until the route is updated or removed.
#[utoipa::path(
    delete,
    path = "/points",
    params(
        DeletePointQuery
    ),
    responses(
        (status = 200, description = "Point deleted successfully", body = Message<Point>),
        (status = 400, description = "Bad request - Point ID is required or point is referenced by existing routes"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Point not found")
    ),
    tag = "Points"
)]
pub async fn delete_points(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Query(query): Query<DeletePointQuery>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point_id = query
        .id
        .ok_or_else(|| ApiError::BadRequest("Point ID is required"))?;
    let point = services::points::delete_point(state, point_id, claims.user_id).await?;
    let response = Message::context("Point deleted successfully.", point);
    Ok(Json(response))
}

/// Get bus stop / terminal point details by ID
///
/// Retrieves the full profile, geo-coordinates, and landmark data for a specific transit stop.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, etc.).
#[utoipa::path(
    get,
    path = "/points/{point_id}",
    params(
        ("point_id" = Uuid, Path, description = "Unique UUID of the transit point to retrieve")
    ),
    responses(
        (status = 200, description = "Point retrieved successfully", body = Message<Point>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 404, description = "Point not found")
    ),
    tag = "Points"
)]
pub async fn get_point_by_id(
    State(state): State<Arc<AppState>>,
    Path(point_id): Path<Uuid>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point = services::points::get_point_by_id(state, point_id).await?;
    let response = Message::context("Point retrieved successfully.", point);
    Ok(Json(response))
}
