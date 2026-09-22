use super::*;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/points", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            get(get_points)
                .post(create_points)
                .patch(update_points)
                .delete(delete_points),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::admin_authorize,
        )))
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Get all points with optional search and pagination
#[utoipa::path(
    get,
    path = "/points",
    params(
        GetPointsQuery
    ),
    responses(
        (status = 200, description = "Points retrieved successfully", body = Message<Vec<Point>>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
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

/// Create a new point
#[utoipa::path(
    post,
    path = "/points",
    request_body = CreatePointDto,
    responses(
        (status = 200, description = "Point created successfully", body = Message<Point>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Points"
)]
pub async fn create_points(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePointDto>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point = services::points::create_point(state, payload).await?;
    let response = Message::context("Point created successfully.", point);
    Ok(Json(response))
}

/// Update an existing point
#[utoipa::path(
    patch,
    path = "/points",
    params(
        ("id" = Option<Uuid>, Query, description = "Point ID to update (optional if provided in body)")
    ),
    request_body = UpdatePointDto,
    responses(
        (status = 200, description = "Point updated successfully", body = Message<Point>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Point not found")
    ),
    tag = "Points"
)]
pub async fn update_points(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PointIdQuery>,
    Json(payload): Json<UpdatePointDto>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point_id = query
        .id
        .or(payload.id)
        .ok_or_else(|| ApiError::BadRequest("Point ID is required"))?;
    let update_payload = UpdatePointPayload {
        name: payload.name,
        geo_location: payload.geo_location,
        landmark: payload.landmark,
        description: payload.description,
    };
    let point = services::points::update_point(state, point_id, update_payload).await?;
    let response = Message::context("Point updated successfully.", point);
    Ok(Json(response))
}

/// Delete a point
#[utoipa::path(
    delete,
    path = "/points",
    params(
        DeletePointQuery
    ),
    responses(
        (status = 200, description = "Point deleted successfully", body = Message<Point>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Point not found")
    ),
    tag = "Points"
)]
pub async fn delete_points(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DeletePointQuery>,
) -> Result<Json<Message<Point>>, ApiError> {
    let point_id = query
        .id
        .ok_or_else(|| ApiError::BadRequest("Point ID is required"))?;
    let point = services::points::delete_point(state, point_id).await?;
    let response = Message::context("Point deleted successfully.", point);
    Ok(Json(response))
}