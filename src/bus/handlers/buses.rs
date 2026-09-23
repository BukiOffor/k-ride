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

/// Register a new bus in the fleet (Admin only)
///
/// Adds a new vehicle into the transit fleet with its seating capacity, registration number, model, and optional route assignment.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Request Body
/// - `registration_number`: Unique vehicle license plate / registration.
/// - `capacity`: Total passenger seating capacity (e.g. 14, 30, 60).
/// - `route_id`: Optional initial route to assign the bus to.
/// - `model` & `amenities`: Vehicle manufacturer, year, and features (e.g. AC, WiFi).
#[utoipa::path(
    post,
    path = "/buses",
    request_body = CreateBusDto,
    responses(
        (status = 200, description = "Bus created successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Validation error or duplicate registration number"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
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
/// Retrieves fleet vehicles with optional filtering by operational state, service status, assigned route, and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, `Driver`, etc.).
///
/// ### Query Parameters
/// - `state`: Operational movement state (`Transit`, `Arrived`, `Stationary`, `Boarding`).
/// - `status`: Mechanical / availability status (`Available`, `Maintenance`, `InUse`).
/// - `route_id`: Filter buses currently allocated to a specific route UUID.
/// - `page` & `limit`: Pagination controls.
#[utoipa::path(
    get,
    path = "/buses",
    params(
        GetBusesQuery
    ),
    responses(
        (status = 200, description = "Buses retrieved successfully", body = Message<Vec<Bus>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token")
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
/// Retrieves full operational profile for a single bus, including seating capacity, current occupied seats, state, status, and assigned route.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role: `User`, `Admin`, `Driver`, etc.).
#[utoipa::path(
    get,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus to retrieve")
    ),
    responses(
        (status = 200, description = "Bus retrieved successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
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
/// Modifies vehicle specifications, registration plate, amenities, or notes.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus to update")
    ),
    request_body = UpdateBusDto,
    responses(
        (status = 200, description = "Bus updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Invalid payload data"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
///
/// ### Constraints
/// - Buses with active bookings cannot be deleted until bookings are fulfilled or cancelled.
#[utoipa::path(
    delete,
    path = "/buses/{bus_id}",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus to delete")
    ),
    responses(
        (status = 200, description = "Bus deleted successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Batch reset all bus routes and allocations (Admin only)
///
/// End-of-day or shift-reset routine that clears all assigned routes, resets passenger counts to 0, sets movement state to `Stationary`, and marks status as `Available` across all buses in the fleet.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Returns
/// Count of fleet buses successfully reset.
#[utoipa::path(
    post,
    path = "/buses/reset-routes",
    responses(
        (status = 200, description = "All bus routes reset successfully", body = Message<usize>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
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

/// Update bus operational movement state (Admin only)
///
/// Updates the real-time operational trip state of the bus.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### States
/// - `Stationary`: Bus parked at terminus or depot.
/// - `Boarding`: Doors open, accepting confirmed passengers.
/// - `Transit`: In transit en route to destination.
/// - `Arrived`: Arrived at destination station.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/state",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus whose state is being updated")
    ),
    request_body = ChangeBusStateDto,
    responses(
        (status = 200, description = "Bus state updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Invalid state value"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Assign or change bus route (Admin only)
///
/// Assigns a bus to a specific active route, or unassigns it by passing null/empty route ID.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Behavior
/// - Validates that the referenced route exists and is active.
/// - Attaches the route UUID to the bus record.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/route",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    request_body = ChangeBusRouteDto,
    responses(
        (status = 200, description = "Bus route updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Route does not exist or is inactive"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
        (status = 404, description = "Bus not found")
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

/// Update bus availability status (Admin only)
///
/// Updates the mechanical / deployment status of the bus.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Status Options
/// - `Available`: Ready for passenger boarding or route assignment.
/// - `Maintenance`: Grounded for mechanical inspection or servicing.
/// - `InUse`: Active on an ongoing trip.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/status",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    request_body = ChangeBusStatusDto,
    responses(
        (status = 200, description = "Bus status updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Invalid status value"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Reset single bus state and route assignment (Admin only)
///
/// Clears the bus passenger count to 0, detaches any active route assignment, sets state to `Stationary`, and resets status to `Available`.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/reset",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus to reset")
    ),
    responses(
        (status = 200, description = "Bus reset successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
/// Overrides or sets the current count of boarded passengers on a bus.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Validation
/// - The `passenger_count` cannot be negative and cannot exceed the bus's configured maximum `capacity`.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/allocation",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    request_body = ChangeBusAllocationDto,
    responses(
        (status = 200, description = "Bus allocation updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Passenger count exceeds bus capacity"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
/// Changes the total available seats on a bus.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Validation
/// - New capacity must be greater than zero and cannot be less than the current active passenger count.
#[utoipa::path(
    patch,
    path = "/buses/{bus_id}/capacity",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    request_body = ChangeBusCapacityDto,
    responses(
        (status = 200, description = "Bus capacity updated successfully", body = Message<Bus>),
        (status = 400, description = "Bad request - Capacity cannot be less than current passenger occupancy"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
/// Disembarks all passengers and resets the bus `passenger_count` to 0.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/buses/{bus_id}/clear-allocation",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    responses(
        (status = 200, description = "Bus allocation cleared successfully", body = Message<Bus>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
