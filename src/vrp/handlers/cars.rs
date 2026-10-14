use super::*;
use crate::auth::jwt::Claims;
use axum::middleware;
use tower::ServiceBuilder;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/cars", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            post(create_car)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_cars),
        )
        .route(
            "/{car_id}",
            patch(update_car)
                .delete(delete_car)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .get(get_car_by_id),
        )
        .route(
            "/{car_id}/assign-user",
            post(assign_car_to_user)
                .delete(unassign_car_from_user)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                )),
        )
        .route(
            "/{car_id}/unassign-user",
            post(unassign_car_from_user).route_layer(middleware::from_fn_with_state(
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

/// Register a new car in the fleet (Admin only)
///
/// Adds a new vehicle into the VRP fleet with its specifications and capacity.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/cars",
    request_body = CreateCarDto,
    responses(
        (status = 200, description = "Car created successfully", body = Message<Car>),
        (status = 400, description = "Bad request - Validation error or duplicate registration"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Admin or Root role required")
    ),
    tag = "Cars"
)]
pub async fn create_car(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateCarDto>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car = services::cars::create_car(state, payload, claims.user_id).await?;
    let response = Message::context("Car created successfully.", car);
    Ok(Json(response))
}

/// List and filter cars in the fleet
///
/// Retrieves fleet vehicles with optional filtering by type, tier, size, verification status, and pagination.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/cars",
    params(GetCarsQuery),
    responses(
        (status = 200, description = "Cars retrieved successfully", body = Message<Vec<Car>>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Cars"
)]
pub async fn get_cars(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetCarsQuery>,
) -> Result<Json<Message<Vec<Car>>>, ApiError> {
    let cars = services::cars::get_cars(state, query).await?;
    let response = Message::context("Cars retrieved successfully.", cars);
    Ok(Json(response))
}

/// Get car details by ID
///
/// Retrieves full specifications and operational profile for a single vehicle.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated role).
#[utoipa::path(
    get,
    path = "/cars/{car_id}",
    params(("car_id" = Uuid, Path, description = "UUID of the car")),
    responses(
        (status = 200, description = "Car retrieved successfully", body = Message<Car>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Car not found")
    ),
    tag = "Cars"
)]
pub async fn get_car_by_id(
    State(state): State<Arc<AppState>>,
    Path(car_id): Path<Uuid>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car = services::cars::get_car_by_id(state, car_id).await?;
    let response = Message::context("Car retrieved successfully.", car);
    Ok(Json(response))
}

/// Update car metadata (Admin only)
///
/// Modifies vehicle specifications, registration plate, or status.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/cars/{car_id}",
    params(("car_id" = Uuid, Path, description = "UUID of the car to update")),
    request_body = UpdateCarDto,
    responses(
        (status = 200, description = "Car updated successfully", body = Message<Car>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Car not found")
    ),
    tag = "Cars"
)]
pub async fn update_car(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(car_id): Path<Uuid>,
    Json(payload): Json<UpdateCarDto>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car = services::cars::update_car(state, car_id, payload, claims.user_id).await?;
    let response = Message::context("Car updated successfully.", car);
    Ok(Json(response))
}

/// Remove a car from the fleet (Admin only)
///
/// Permanently deletes a car record from the fleet management system and unassigns it from any user.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    delete,
    path = "/cars/{car_id}",
    params(("car_id" = Uuid, Path, description = "UUID of the car to delete")),
    responses(
        (status = 200, description = "Car deleted successfully", body = Message<Car>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Car not found")
    ),
    tag = "Cars"
)]
pub async fn delete_car(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(car_id): Path<Uuid>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car = services::cars::delete_car(state, car_id, claims.user_id).await?;
    let response = Message::context("Car deleted successfully.", car);
    Ok(Json(response))
}

/// Assign car to a user (Admin only)
///
/// Assigns a car to a specific user (driver). Also unassigns the car from any previous user.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    post,
    path = "/cars/{car_id}/assign-user",
    params(("car_id" = Uuid, Path, description = "UUID of the car")),
    request_body = AssignCarToUserDto,
    responses(
        (status = 200, description = "Car assigned to user successfully", body = Message<Car>),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Car or user not found")
    ),
    tag = "Cars"
)]
pub async fn assign_car_to_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(car_id): Path<Uuid>,
    Json(payload): Json<AssignCarToUserDto>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car =
        services::cars::assign_car_to_user(state, car_id, payload.user_id, claims.user_id).await?;
    let response = Message::context("Car assigned to user successfully.", car);
    Ok(Json(response))
}

/// Unassign car from user (Admin only)
///
/// Unassigns a car from whatever user currently has it assigned.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    delete,
    path = "/cars/{car_id}/assign-user",
    params(("car_id" = Uuid, Path, description = "UUID of the car")),
    responses(
        (status = 200, description = "Car unassigned successfully", body = Message<Car>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Car not found")
    ),
    tag = "Cars"
)]
pub async fn unassign_car_from_user(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(car_id): Path<Uuid>,
) -> Result<Json<Message<Car>>, ApiError> {
    let car = services::cars::unassign_car_from_user(state, car_id, claims.user_id).await?;
    let response = Message::context("Car unassigned successfully.", car);
    Ok(Json(response))
}