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
            "/reset-trips",
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
                    crate::auth::middleware::coordinator_authorize,
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
            "/{bus_id}/trip",
            patch(change_bus_trip).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/assign-trip",
            post(assign_bus_to_trip).route_layer(middleware::from_fn_with_state(
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
        .route(
            "/{bus_id}/increment-passenger",
            post(increment_passenger_count).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{bus_id}/decrement-passenger",
            post(decrement_passenger_count).route_layer(middleware::from_fn_with_state(
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

/// Register a new bus in the fleet (Admin only)
///
/// Adds a new vehicle into the transit fleet with its seating capacity and registration number.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses",
    request_body = CreateBusDto,
    responses(
        (status = 200, description = "Bus created successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Validation error or duplicate registration number"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin or Root role required")
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

/// List and filter buses in the fleet
///
/// Retrieves fleet vehicles with optional filtering by operational state, service status, assigned trip, and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/buses",
    params(GetBusesQuery),
    responses(
        (status = 200, description = "Buses retrieved successfully", body = Message<Vec<Bus>>),
        (status = 401, description = "Unauthorized")
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

/// Get bus details by ID
///
/// Retrieves full operational profile for a single bus, including seating capacity, current occupied seats, state, status, and assigned trip.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/buses/{bus_id}",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    responses(
        (status = 200, description = "Bus retrieved successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Bus not found")
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

/// Update bus fleet metadata (Admin only)
///
/// Modifies vehicle specifications, registration plate, or notes.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus to update")),
    request_body = UpdateBusDto,
    responses(
        (status = 200, description = "Bus updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Remove a bus from the fleet (Admin only)
///
/// Permanently deletes a bus record from the fleet management system.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    delete,
    path = "/buses/{bus_id}",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus to delete")),
    responses(
        (status = 200, description = "Bus deleted successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Batch reset all bus trip assignments and allocations (Admin only)
///
/// End-of-day or shift-reset routine that clears all assigned trips, resets passenger counts to 0.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses/reset-trips",
    responses(
        (status = 200, description = "All bus trips reset successfully", body = Message<usize>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    tag = "Buses"
)]
pub async fn reset_all_bus_routes(
    State(state): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<Message<usize>>, ApiError> {
    let count = services::buses::reset_all_bus_routes(state, claims.user_id).await?;
    let response = Message::context("All bus trips reset successfully.", count);
    Ok(Json(response))
}

/// Update bus operational movement state (Admin only)
///
/// Updates the real-time operational state of the bus (`Stationary`, `Boarding`, `Transit`, `Arrived`).
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/state",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = ChangeBusStateDto,
    responses(
        (status = 200, description = "Bus state updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Change or unassign bus trip (Admin only)
///
/// Assigns a bus to a specific trip, or unassigns it by passing null trip ID.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/trip",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = ChangeBusTripDto,
    responses(
        (status = 200, description = "Bus trip updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Trip does not exist"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
    ),
    tag = "Buses"
)]
pub async fn change_bus_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<ChangeBusTripDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus =
        services::buses::change_bus_trip(state, bus_id, payload.trip_id, claims.user_id).await?;
    let response = Message::context("Bus trip updated successfully.", bus);
    Ok(Json(response))
}

/// Update bus availability status (Admin only)
///
/// Updates the mechanical / deployment status (`Available`, `Maintenance`, `InUse`).
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/status",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = ChangeBusStatusDto,
    responses(
        (status = 200, description = "Bus status updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Reset single bus state and trip assignment (Admin only)
///
/// Clears the bus passenger count to 0, detaches any active trip assignment, sets state to `Stationary`, and resets status to `Available`.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/reset",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus to reset")),
    responses(
        (status = 200, description = "Bus reset successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Manually adjust bus passenger occupancy count (Admin only)
///
/// Overrides the current count of boarded passengers on a bus. Cannot exceed capacity.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/allocation",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = ChangeBusAllocationDto,
    responses(
        (status = 200, description = "Bus allocation updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Passenger count exceeds bus capacity"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Update bus maximum seating capacity (Admin only)
///
/// Changes the total available seats on a bus. Cannot be less than current passenger count.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/capacity",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = ChangeBusCapacityDto,
    responses(
        (status = 200, description = "Bus capacity updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Clear bus passenger occupancy to zero (Admin only)
///
/// Disembarks all passengers, resets passenger count to 0, and unassigns the current trip.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/clear-allocation",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    responses(
        (status = 200, description = "Bus allocation cleared successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus not found")
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

/// Assign bus to a trip (Admin only)
///
/// Assigns a bus to a specific scheduled trip. Also updates the trip's bus_id bidirectionally.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Validates that the bus exists and is not under maintenance.
/// - Validates that the trip exists.
/// - Sets `current_trip_id` on the bus and `bus_id` on the trip.
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/assign-trip",
    params(("bus_id" = Uuid, Path, description = "UUID of the bus")),
    request_body = AssignBusToTripDto,
    responses(
        (status = 200, description = "Bus assigned to trip successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Trip does not exist or bus is under maintenance"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Bus or trip not found")
    ),
    tag = "Buses"
)]
pub async fn assign_bus_to_trip(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
    Json(payload): Json<AssignBusToTripDto>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus =
        services::buses::assign_bus_to_trip(state, bus_id, payload.trip_id, claims.user_id).await?;
    let response = Message::context("Bus assigned to trip successfully.", bus);
    Ok(Json(response))
}
// increment bus current_passenger_count by 1 
pub async fn increment_passenger_count(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::get_bus_by_id(state.clone(), bus_id).await?;
    let updated_bus = services::buses::change_bus_allocation(
        state,
        bus_id,
        bus.current_passenger_count + 1,
        claims.user_id,
    )
    .await?;
    let response = Message::context("Passenger count incremented successfully", updated_bus);
    Ok(Json(response))
}

// decrease current_passenger_count by 1
pub async fn decrement_passenger_count(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<Bus>>, ApiError> {
    let bus = services::buses::get_bus_by_id(state.clone(), bus_id).await?;
    let new_count = (bus.current_passenger_count - 1).max(0);
    let updated_bus = services::buses::change_bus_allocation(
        state,
        bus_id,
        new_count,
        claims.user_id,
    )
    .await?;
    let response = Message::context("Passenger count decremented successfully", updated_bus);
    Ok(Json(response))
}