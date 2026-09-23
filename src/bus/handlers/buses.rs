use super::*;
use crate::auth::jwt::Claims;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/buses", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            post(create_bus)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_buses),
        )
        .route(
            "/reset-routes",
            post(reset_all_bus_routes).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}",
            patch(update_bus)
                .delete(delete_bus)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_bus_by_id),
        )
        .route(
            "/{bus_id}/state",
            patch(change_bus_state).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/route",
            patch(change_bus_route).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/status",
            patch(change_bus_status).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/reset",
            post(reset_bus).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/allocation",
            patch(change_bus_allocation)
                .delete(clear_bus_allocation)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                )),
        )
        .route(
            "/{bus_id}/clear-allocation",
            post(clear_bus_allocation).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/capacity",
            patch(change_bus_capacity).route_layer(middleware::from_fn_with_state(
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

/// Create a new bus
#[utoipa::path(
    post,
    path = "/buses",
    request_body = CreateBusDto,
    responses(
        (status = 200, description = "Bus created successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn create_bus(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateBusDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::create_bus(state, payload, claims.user_id).await?;
    let response = Message::context("Bus created successfully.", bus);
    Ok(Json(response))
}

/// Get all buses with optional filter and pagination
#[utoipa::path(
    get,
    path = "/buses",
    params(
        GetBusesQuery
    ),
    responses(
        (status = 200, description = "Buses retrieved successfully", body = Message<Vec<Bus>>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn get_buses(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetBusesQuery>,
) -> Result<Json<Message<Vec<Bus>>>, ApiError> {
    let buses = services::buses::get_buses(state, query).await?;
    let response = Message::context("Buses retrieved successfully.", buses);
    Ok(Json(response))
}

/// Get bus by ID
#[utoipa::path(
    get,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    responses(
        (status = 200, description = "Bus retrieved successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn get_bus_by_id(
    State(state): State<Arc<AppState>>,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::get_bus_by_id(state, bus_id).await?;
    let response = Message::context("Bus retrieved successfully.", bus);
    Ok(Json(response))
}

/// Update a bus
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = UpdateBusDto,
    responses(
        (status = 200, description = "Bus updated successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn update_bus(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<UpdateBusDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::update_bus(state, bus_id, payload, claims.user_id).await?;
    let response = Message::context("Bus updated successfully.", bus);
    Ok(Json(response))
}

/// Delete a bus
#[utoipa::path(
    delete,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    responses(
        (status = 200, description = "Bus deleted successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn delete_bus(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::delete_bus(state, bus_id, claims.user_id).await?;
    let response = Message::context("Bus deleted successfully.", bus);
    Ok(Json(response))
}

/// Reset all bus routes and allocations
#[utoipa::path(
    post,
    path = "/buses/reset-routes",
    responses(
        (status = 200, description = "Bus routes reset successfully", body = Message<usize>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn reset_all_bus_routes(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<Message<usize>>, ApiError> {
    let count = services::buses::reset_all_bus_routes(state, claims.user_id).await?;
    let response = Message::context("All bus routes reset successfully.", count);
    Ok(Json(response))
}

/// Change bus state (Transit, Arrived, Stationary, Boarding)
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/state",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = ChangeBusStateDto,
    responses(
        (status = 200, description = "Bus state updated successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn change_bus_state(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusStateDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus =
        services::buses::change_bus_state(state, bus_id, payload.state, claims.user_id).await?;
    let response = Message::context("Bus state updated successfully.", bus);
    Ok(Json(response))
}

/// Change bus route assignment
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/route",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = ChangeBusRouteDto,
    responses(
        (status = 200, description = "Bus route updated successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn change_bus_route(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusRouteDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus =
        services::buses::change_bus_route(state, bus_id, payload.route_id, claims.user_id).await?;
    let response = Message::context("Bus route updated successfully.", bus);
    Ok(Json(response))
}

/// Change bus status (Available, Maintenance, InUse)
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/status",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = ChangeBusStatusDto,
    responses(
        (status = 200, description = "Bus status updated successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn change_bus_status(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusStatusDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus =
        services::buses::change_bus_status(state, bus_id, payload.status, claims.user_id).await?;
    let response = Message::context("Bus status updated successfully.", bus);
    Ok(Json(response))
}

/// Reset a single bus (clears passenger count, route, sets state to Stationary and status to Available)
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/reset",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    responses(
        (status = 200, description = "Bus reset successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn reset_bus(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::reset_bus(state, bus_id, claims.user_id).await?;
    let response = Message::context("Bus reset successfully.", bus);
    Ok(Json(response))
}

/// Change bus passenger allocation
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/allocation",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = ChangeBusAllocationDto,
    responses(
        (status = 200, description = "Bus allocation updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn change_bus_allocation(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusAllocationDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::change_bus_allocation(
        state,
        bus_id,
        payload.passenger_count,
        claims.user_id,
    )
    .await?;
    let response = Message::context("Bus allocation updated successfully.", bus);
    Ok(Json(response))
}

/// Change bus capacity
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/capacity",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    request_body = ChangeBusCapacityDto,
    responses(
        (status = 200, description = "Bus capacity updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn change_bus_capacity(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusCapacityDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::change_bus_capacity(state, bus_id, payload.capacity, claims.user_id)
        .await?;
    let response = Message::context("Bus capacity updated successfully.", bus);
    Ok(Json(response))
}

/// Clear bus current allocation (sets passenger count to 0)
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/clear-allocation",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    responses(
        (status = 200, description = "Bus allocation cleared successfully", body = Message<Bus>),
        (status = 404, description = "Bus not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Buses"
)]
pub async fn clear_bus_allocation(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::clear_bus_allocation(state, bus_id, claims.user_id).await?;
    let response = Message::context("Bus allocation cleared successfully.", bus);
    Ok(Json(response))
}
