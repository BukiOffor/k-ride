use super::*;
use crate::auth::jwt::Claims;
use crate::bus::models::routes::Route;
use crate::Message;

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
                crate::auth::middleware::admin_authorize,
            )),
        )
        .layer(ServiceBuilder::new().layer(middleware::from_fn_with_state(
            state.clone(),
            crate::auth::middleware::authorize,
        )))
        .with_state(state)
}

/// Create a new route (admin only; if marked as round trip, auto-generates return route)
#[utoipa::path(
    post,
    path = "/routes",
    request_body = CreateRouteDto,
    responses(
        (status = 200, description = "Route created successfully", body = Message<Vec<Route>>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
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

/// Get all routes with optional search, filters, and pagination
#[utoipa::path(
    get,
    path = "/routes",
    params(
        GetRoutesQuery
    ),
    responses(
        (status = 200, description = "Routes retrieved successfully", body = Message<Vec<Route>>),
        (status = 401, description = "Unauthorized")
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

/// Get route by ID
#[utoipa::path(
    get,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Route ID")
    ),
    responses(
        (status = 200, description = "Route retrieved successfully", body = Message<Route>),
        (status = 404, description = "Route not found"),
        (status = 401, description = "Unauthorized")
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

/// Update a route (admin only)
#[utoipa::path(
    patch,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Route ID")
    ),
    request_body = UpdateRouteDto,
    responses(
        (status = 200, description = "Route updated successfully", body = Message<Route>),
        (status = 404, description = "Route not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
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

/// Delete a route (admin only)
#[utoipa::path(
    delete,
    path = "/routes/{route_id}",
    params(
        ("route_id" = Uuid, Path, description = "Route ID")
    ),
    responses(
        (status = 200, description = "Route deleted successfully", body = Message<Route>),
        (status = 404, description = "Route not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
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

/// Mark route as completed and set completion time (admin only)
#[utoipa::path(
    patch,
    path = "/routes/{route_id}/complete",
    params(
        ("route_id" = Uuid, Path, description = "Route ID")
    ),
    responses(
        (status = 200, description = "Route marked as completed successfully", body = Message<Route>),
        (status = 404, description = "Route not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
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