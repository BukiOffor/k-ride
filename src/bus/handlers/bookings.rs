use super::*;
use crate::Message;
use crate::auth::jwt::Claims;
use crate::users::RoleType;

pub fn routes(state: Arc<AppState>) -> Router {
    let routes = module_routes(state.clone());
    let api = Router::new().nest("/bookings", routes);
    Router::new().merge(api)
}

pub fn module_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            get(get_bookings)
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    crate::auth::middleware::admin_authorize,
                ))
                .post(create_booking),
        )
        .route("/me", get(get_my_bookings))
        .route("/bus/{bus_id}/seats", get(get_bus_seat_status))
        .route(
            "/{booking_id}",
            get(get_booking_by_id).delete(cancel_booking),
        )
        .route("/{booking_id}/cancel", post(cancel_booking))
        .route(
            "/{booking_id}/board",
            patch(board_passenger).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/{booking_id}/payment",
            patch(update_payment_status).route_layer(middleware::from_fn_with_state(
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

/// Create a booking (locks seat to prevent double-booking and increments passenger count)
#[utoipa::path(
    post,
    path = "/bookings",
    request_body = CreateBookingDto,
    responses(
        (status = 200, description = "Booking created successfully", body = Message<Booking>),
        (status = 400, description = "Bad request (bus full, seat taken, or invalid parameters)"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Route or bus not found")
    ),
    tag = "Bookings"
)]
pub async fn create_booking(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Json(payload): Json<CreateBookingDto>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let is_admin = matches!(claims.role, RoleType::Admin | RoleType::Root);
    let booking =
        services::bookings::create_booking(state, payload, claims.user_id, is_admin).await?;
    let response = Message::context("Booking created successfully.", booking);
    Ok(Json(response))
}

/// Cancel a booking (decrements bus passenger count)
#[utoipa::path(
    delete,
    path = "/bookings/{booking_id}",
    params(
        ("booking_id" = Uuid, Path, description = "Booking ID")
    ),
    responses(
        (status = 200, description = "Booking cancelled successfully", body = Message<Booking>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn cancel_booking(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let is_admin = matches!(claims.role, RoleType::Admin | RoleType::Root);
    let booking =
        services::bookings::cancel_booking(state, booking_id, claims.user_id, is_admin).await?;
    let response = Message::context("Booking cancelled successfully.", booking);
    Ok(Json(response))
}

/// Get my bookings
#[utoipa::path(
    get,
    path = "/bookings/me",
    params(
        GetBookingsQuery
    ),
    responses(
        (status = 200, description = "My bookings retrieved successfully", body = Message<Vec<Booking>>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "Bookings"
)]
pub async fn get_my_bookings(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Query(query): Query<GetBookingsQuery>,
) -> Result<Json<Message<Vec<Booking>>>, ApiError> {
    let bookings = services::bookings::get_my_bookings(state, claims.user_id, query).await?;
    let response = Message::context("My bookings retrieved successfully.", bookings);
    Ok(Json(response))
}

/// Get bus seat availability map
#[utoipa::path(
    get,
    path = "/bookings/bus/{bus_id}/seats",
    params(
        ("bus_id" = Uuid, Path, description = "Bus ID")
    ),
    responses(
        (status = 200, description = "Bus seat map retrieved successfully", body = Message<BusSeatsResponseDto>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Bus not found")
    ),
    tag = "Bookings"
)]
pub async fn get_bus_seat_status(
    State(state): State<Arc<AppState>>,
    Path(bus_id): Path<Uuid>,
) -> Result<Json<Message<BusSeatsResponseDto>>, ApiError> {
    let seat_status = services::bookings::get_bus_seat_status(state, bus_id).await?;
    let response = Message::context("Bus seat status retrieved successfully.", seat_status);
    Ok(Json(response))
}

/// Get booking by ID
#[utoipa::path(
    get,
    path = "/bookings/{booking_id}",
    params(
        ("booking_id" = Uuid, Path, description = "Booking ID")
    ),
    responses(
        (status = 200, description = "Booking retrieved successfully", body = Message<Booking>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn get_booking_by_id(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let is_admin = matches!(claims.role, RoleType::Admin | RoleType::Root);
    let booking =
        services::bookings::get_booking_by_id(state, booking_id, claims.user_id, is_admin).await?;
    let response = Message::context("Booking retrieved successfully.", booking);
    Ok(Json(response))
}

/// Get all bookings with filters (Admin only)
#[utoipa::path(
    get,
    path = "/bookings",
    params(
        GetBookingsQuery
    ),
    responses(
        (status = 200, description = "Bookings retrieved successfully", body = Message<Vec<Booking>>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Bookings"
)]
pub async fn get_bookings(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetBookingsQuery>,
) -> Result<Json<Message<Vec<Booking>>>, ApiError> {
    let bookings = services::bookings::get_bookings(state, query).await?;
    let response = Message::context("Bookings retrieved successfully.", bookings);
    Ok(Json(response))
}

/// Board a passenger (Admin only)
#[utoipa::path(
    patch,
    path = "/bookings/{booking_id}/board",
    params(
        ("booking_id" = Uuid, Path, description = "Booking ID")
    ),
    responses(
        (status = 200, description = "Passenger boarded successfully", body = Message<Booking>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn board_passenger(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let booking = services::bookings::board_passenger(state, booking_id, claims.user_id).await?;
    let response = Message::context("Passenger boarded successfully.", booking);
    Ok(Json(response))
}

/// Update booking payment status (Admin only)
#[utoipa::path(
    patch,
    path = "/bookings/{booking_id}/payment",
    params(
        ("booking_id" = Uuid, Path, description = "Booking ID")
    ),
    request_body = UpdatePaymentStatusDto,
    responses(
        (status = 200, description = "Payment status updated successfully", body = Message<Booking>),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn update_payment_status(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
    Json(payload): Json<UpdatePaymentStatusDto>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let booking = services::bookings::update_payment_status(
        state,
        booking_id,
        payload.payment_status,
        claims.user_id,
    )
    .await?;
    let response = Message::context("Payment status updated successfully.", booking);
    Ok(Json(response))
}
