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
                    crate::auth::middleware::coordinator_authorize,
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
        .route("/{booking_id}/qrcode", get(get_booking_qr))
        .route(
            "/{booking_id}/board",
            patch(board_passenger).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::coordinator_authorize,
            )),
        )
        .route(
            "/{booking_id}/payment",
            patch(update_payment_status).route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::auth::middleware::admin_authorize,
            )),
        )
        .route(
            "/verify",
            post(verify_booking_qr).route_layer(middleware::from_fn_with_state(
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

/// Book a seat on a scheduled bus trip
///
/// Reserves a seat on a bus for an itinerary. Supports both automatic seat allocation and specific seat selection.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
/// - Standard customers book for themselves (`claims.user_id`).
/// - Users with `Admin` or `Root` roles can optionally provide a different `user_id` to book on behalf of a passenger.
///
/// ### Concurrency & Seat Allocation
/// - **Row-Level Lock:** Performs a PostgreSQL `FOR UPDATE` lock on the bus record to prevent race conditions and overbooking.
/// - **Automatic Allocation:** If `seat_number` is omitted, the system dynamically assigns the next lowest unoccupied seat (from 1 up to maximum bus capacity).
/// - **Manual Selection:** If a specific `seat_number` is requested, the system verifies that the seat is within capacity bounds and not already booked by another active passenger.
/// - **Passenger Count:** Automatically increments the bus `passenger_count`.
#[utoipa::path(
    post,
    path = "/bookings",
    request_body = CreateBookingDto,
    responses(
        (status = 200, description = "Booking created successfully", body = Message<Booking>),
        (status = 400, description = "Bad request - User already has an active booking for this route, bus is at maximum capacity, seat is already taken, or invalid parameters"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
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

/// Cancel a bus reservation
///
/// Cancels an existing booking, frees up the occupied seat for other passengers, and decrements the bus passenger occupancy.
///
/// ### Access Control
/// - **Customer Cancellation:** Passengers can cancel their own bookings while in `Pending` or `Confirmed` status.
/// - **Admin Cancellation:** Users with `Admin` or `Root` roles can cancel any booking.
///
/// ### Side Effects
/// - Updates booking status to `Cancelled`.
/// - Atomically decrements the bus `passenger_count`.
#[utoipa::path(
    delete,
    path = "/bookings/{booking_id}",
    params(
        ("booking_id" = Uuid, Path, description = "Unique UUID of the booking to cancel")
    ),
    responses(
        (status = 200, description = "Booking cancelled successfully", body = Message<Booking>),
        (status = 400, description = "Bad request - Cannot cancel an already boarded or completed booking"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Cannot cancel another passenger's booking"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn cancel_booking(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let is_admin = matches!(
        claims.role,
        RoleType::Admin | RoleType::Root | RoleType::RouteCoordinator
    );
    let booking =
        services::bookings::cancel_booking(state, booking_id, claims.user_id, is_admin).await?;
    let response = Message::context("Booking cancelled successfully.", booking);
    Ok(Json(response))
}

/// View personal booking history
///
/// Retrieves all bookings belonging to the currently authenticated passenger, sorted by creation date.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
///
/// ### Query Parameters
/// - `status`: Filter by booking status (`Pending`, `Confirmed`, `Cancelled`, `Boarded`, `Completed`).
/// - `page` & `limit`: Pagination controls.
#[utoipa::path(
    get,
    path = "/bookings/me",
    params(
        GetBookingsQuery
    ),
    responses(
        (status = 200, description = "Personal bookings retrieved successfully", body = Message<Vec<Booking>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token")
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

/// Get real-time seat availability map for a bus
///
/// Returns the seat layout and occupancy details for a specific bus, enabling passengers to visualize and select vacant seats.
///
/// ### Access Control
/// - **Requires:** Bearer JWT (any authenticated user).
///
/// ### Response Structure
/// - `capacity`: Total number of seats on the bus.
/// - `occupied_seats`: Array of seat numbers currently booked and unavailable.
/// - `available_seats`: Array of open seat numbers ready for booking.
#[utoipa::path(
    get,
    path = "/bookings/bus/{bus_id}/seats",
    params(
        ("bus_id" = Uuid, Path, description = "Unique UUID of the bus")
    ),
    responses(
        (status = 200, description = "Bus seat map retrieved successfully", body = Message<BusSeatsResponseDto>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
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

/// Get booking details by ID
///
/// Retrieves complete reservation details including assigned seat, ticket fare, trip schedule, bus assignment, and payment status.
///
/// ### Access Control
/// - **Requires:** The passenger who owns the reservation, or a user with `Admin` / `Root` privileges.
#[utoipa::path(
    get,
    path = "/bookings/{booking_id}",
    params(
        ("booking_id" = Uuid, Path, description = "Unique UUID of the booking")
    ),
    responses(
        (status = 200, description = "Booking retrieved successfully", body = Message<Booking>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Cannot view another user's booking"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn get_booking_by_id(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<Booking>>, ApiError> {
    let is_admin = matches!(
        claims.role,
        RoleType::Admin | RoleType::Root | RoleType::RouteCoordinator
    );
    let booking =
        services::bookings::get_booking_by_id(state, booking_id, claims.user_id, is_admin).await?;
    let response = Message::context("Booking retrieved successfully.", booking);
    Ok(Json(response))
}

/// List and filter all bookings (Admin only)
///
/// Administrative overview of all passenger bookings across routes, buses, and users.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Query Parameters
/// - `bus_id`: Filter by assigned bus UUID.
/// - `route_id`: Filter by scheduled route UUID.
/// - `user_id`: Filter by passenger account UUID.
/// - `status`: Filter by status (`Pending`, `Confirmed`, `Cancelled`, `Boarded`, `Completed`).
/// - `page` & `limit`: Pagination controls.
#[utoipa::path(
    get,
    path = "/bookings",
    params(
        GetBookingsQuery
    ),
    responses(
        (status = 200, description = "Bookings retrieved successfully", body = Message<Vec<Booking>>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
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

/// Check-in and board a passenger (Admin / Conductor only)
///
/// Records ticket validation at the vehicle entrance. Transitions the booking status to `Boarded` and logs the timestamp.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role (conductors/inspectors).
#[utoipa::path(
    patch,
    path = "/bookings/{booking_id}/board",
    params(
        ("booking_id" = Uuid, Path, description = "Unique UUID of the booking being boarded")
    ),
    responses(
        (status = 200, description = "Passenger boarded successfully", body = Message<Booking>),
        (status = 400, description = "Bad request - Booking is cancelled or already boarded"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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
///
/// Updates financial transaction settlement for a ticket reservation (`Pending`, `Paid`, `Failed`, `Refunded`).
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
#[utoipa::path(
    patch,
    path = "/bookings/{booking_id}/payment",
    params(
        ("booking_id" = Uuid, Path, description = "Unique UUID of the booking")
    ),
    request_body = UpdatePaymentStatusDto,
    responses(
        (status = 200, description = "Payment status updated successfully", body = Message<Booking>),
        (status = 400, description = "Bad request - Invalid payment status value"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required"),
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

/// Generate a QR code booking pass
///
/// Produces a QR code PNG image (returned as base64-encoded string) that encodes the booking
/// details with a cryptographic HMAC-SHA256 signature. The passenger can present this QR code
/// to the conductor/admin for boarding verification.
///
/// ### Access Control
/// - **Requires:** Bearer JWT. Passengers can generate QR codes for their own bookings.
/// - Admin/Root users can generate QR codes for any booking.
#[utoipa::path(
    get,
    path = "/bookings/{booking_id}/qrcode",
    params(
        ("booking_id" = Uuid, Path, description = "Unique UUID of the booking")
    ),
    responses(
        (status = 200, description = "QR code generated successfully", body = Message<BookingQrCodeResponse>),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Cannot generate QR code for another user's booking"),
        (status = 404, description = "Booking not found")
    ),
    tag = "Bookings"
)]
pub async fn get_booking_qr(
    State(state): State<Arc<AppState>>,
    claims: Claims,
    Path(booking_id): Path<Uuid>,
) -> Result<Json<Message<BookingQrCodeResponse>>, ApiError> {
    let is_admin = matches!(
        claims.role,
        RoleType::Admin | RoleType::Root | RoleType::RouteCoordinator
    );
    let qr = services::bookings::generate_booking_qr(state, booking_id, claims.user_id, is_admin)
        .await?;
    let response = Message::context("QR code generated successfully.", qr);
    Ok(Json(response))
}

/// Verify a scanned QR code booking pass (Admin / Conductor only)
///
/// Accepts the raw JSON string extracted from a scanned QR code, validates its cryptographic
/// signature, looks up the booking in the database, and returns the booking's current status,
/// payment information, and seat assignment.
///
/// ### Access Control
/// - **Requires:** Bearer JWT with `Admin` or `Root` role.
///
/// ### Verification Steps
/// 1. Parse the QR payload JSON.
/// 2. Verify the HMAC-SHA256 signature to ensure the QR code was generated by this system.
/// 3. Look up the booking by ID and return its live status.
#[utoipa::path(
    post,
    path = "/bookings/verify",
    request_body = VerifyBookingDto,
    responses(
        (status = 200, description = "QR code verified", body = Message<BookingVerificationResult>),
        (status = 400, description = "Bad request - Invalid QR code format"),
        (status = 401, description = "Unauthorized - Missing or invalid authentication token"),
        (status = 403, description = "Forbidden - Admin or Root role required")
    ),
    tag = "Bookings"
)]
pub async fn verify_booking_qr(
    State(state): State<Arc<AppState>>,
    _claims: Claims,
    Json(payload): Json<VerifyBookingDto>,
) -> Result<Json<Message<BookingVerificationResult>>, ApiError> {
    let result = services::bookings::verify_booking_qr(state, payload).await?;
    let response = Message::context("Booking verification complete.", result);
    Ok(Json(response))
}
