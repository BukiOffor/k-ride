use super::*;
use crate::logs::{ActivityLog, ActivityType};
use diesel::OptionalExtension;
use diesel::PgTextExpressionMethods;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

// Endpoint-level bus lock: ensures concurrent booking requests on the same bus are serialized
static BUS_LOCKS: LazyLock<Mutex<HashMap<Uuid, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

async fn acquire_bus_lock(bus_id: Uuid) -> Arc<Mutex<()>> {
    let mut map = BUS_LOCKS.lock().await;
    map.entry(bus_id)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub async fn create_booking(
    state: Arc<AppState>,
    payload: CreateBookingDto,
    current_user_id: Uuid,
    is_admin: bool,
) -> Result<Booking, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;
    use crate::schema::buses::dsl as buses_dsl;
    use crate::schema::routes::dsl as routes_dsl;

    // 1. Acquire endpoint-level concurrency lock for this bus
    let bus_lock = acquire_bus_lock(payload.bus_id).await;
    let _guard = bus_lock.lock().await;

    let effective_user_id = if is_admin && payload.user_id.is_some() {
        payload.user_id.unwrap()
    } else {
        current_user_id
    };

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // 2. Validate route existence
    let route = routes_dsl::routes
        .filter(routes_dsl::id.eq(payload.route_id))
        .first::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // 3. Verify user does not already have an active booking for this route
    let existing_booking = bookings_dsl::bookings
        .filter(bookings_dsl::user_id.eq(effective_user_id))
        .filter(bookings_dsl::route_id.eq(payload.route_id))
        .filter(bookings_dsl::status.ne(BookingStatus::Cancelled))
        .filter(bookings_dsl::status.ne(BookingStatus::Revoked))
        .first::<Booking>(&mut conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    if existing_booking.is_some() {
        return Err(ApiError::Error(
            "User already has an active booking for this route".into(),
        ));
    }

    // 4. Validate bus existence and status
    let bus = buses_dsl::buses
        .filter(buses_dsl::id.eq(payload.bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // 4. Verify bus is assigned to this route
    if bus.current_route_id != Some(payload.route_id) {
        return Err(ApiError::Error(
            "Bus is not currently assigned to the specified route".into(),
        ));
    }

    // 5. Verify bus availability
    if bus.status == BusStatus::Maintenance {
        return Err(ApiError::Error(
            "Bus is currently under maintenance and cannot be booked".into(),
        ));
    }

    // 6. Verify passenger capacity
    if bus.current_passenger_count >= bus.capacity {
        return Err(ApiError::Error("Bus is already at full capacity".into()));
    }

    // 7. Load all active booked seats on this bus
    let active_bookings = bookings_dsl::bookings
        .filter(bookings_dsl::bus_id.eq(payload.bus_id))
        .filter(bookings_dsl::status.ne(BookingStatus::Cancelled))
        .filter(bookings_dsl::status.ne(BookingStatus::Revoked))
        .select(bookings_dsl::seat_number)
        .load::<i32>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booked_set: HashSet<i32> = active_bookings.iter().cloned().collect();

    // 8. Assign seat number automatically based on the last seat, or validate requested seat
    let assigned_seat: i32 = match payload.seat_number {
        Some(s) => {
            if s < 1 || s > bus.capacity {
                return Err(ApiError::Error(
                    format!("Seat number must be between 1 and {}", bus.capacity).into(),
                ));
            }
            if booked_set.contains(&s) {
                return Err(ApiError::Error(
                    format!("Seat {} on this bus is already booked", s).into(),
                ));
            }
            s
        }
        None => {
            // Find last booked seat and assign next
            let last_seat = active_bookings.iter().max().cloned().unwrap_or(0);
            let next_seat = last_seat + 1;
            if next_seat <= bus.capacity && !booked_set.contains(&next_seat) {
                next_seat
            } else {
                // If next seat exceeds capacity or is taken, pick lowest available seat
                (1..=bus.capacity)
                    .find(|s| !booked_set.contains(s))
                    .ok_or_else(|| ApiError::Error("Bus is already at full capacity".into()))?
            }
        }
    };

    // 9. Build booking record
    let mut new_booking = Booking::new(
        effective_user_id,
        payload.bus_id,
        payload.route_id,
        assigned_seat,
    );

    if route.fare == 0.0 {
        new_booking.payment_status = PaymentStatus::Free;
    }

    // 10. Generate QR code and attach it before insert
    let qr_payload = build_signed_qr_payload(&new_booking)?;
    let qr_base64 = render_qr_to_base64(&qr_payload)?;
    new_booking.qr_code = Some(qr_base64);

    // 11. Insert booking (with QR code already populated)
    let created_booking = diesel::insert_into(bookings_dsl::bookings)
        .values(&new_booking)
        .get_result::<Booking>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // 12. Increment current_passenger_count on the bus
    diesel::update(buses_dsl::buses.filter(buses_dsl::id.eq(payload.bus_id)))
        .set((
            buses_dsl::current_passenger_count.eq(bus.current_passenger_count + 1),
            buses_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // 13. Emit activity log
    let mut log = ActivityLog::new(ActivityType::BookingCreated, current_user_id);
    log.set_target_id(created_booking.id);
    log.set_target_type("Booking".to_string());
    if let Ok(details) = serde_json::to_value(&created_booking) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(created_booking)
}

pub async fn cancel_booking(
    state: Arc<AppState>,
    booking_id: Uuid,
    current_user_id: Uuid,
    is_admin: bool,
) -> Result<Booking, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;
    use crate::schema::buses::dsl as buses_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booking = bookings_dsl::bookings
        .filter(bookings_dsl::id.eq(booking_id))
        .first::<Booking>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Booking not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // Non-admins can only cancel their own booking
    if !is_admin && booking.user_id != current_user_id {
        return Err(ApiError::PermissionDenied);
    }

    if booking.status == BookingStatus::Cancelled {
        return Err(ApiError::Error("Booking is already cancelled".into()));
    }

    if booking.status == BookingStatus::Boarded {
        return Err(ApiError::Error(
            "Cannot cancel a booking that has already boarded".into(),
        ));
    }

    // Update status to Cancelled
    let updated_booking =
        diesel::update(bookings_dsl::bookings.filter(bookings_dsl::id.eq(booking_id)))
            .set((
                bookings_dsl::status.eq(BookingStatus::Cancelled),
                bookings_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
            ))
            .get_result::<Booking>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Decrement current_passenger_count on the bus
    let bus = buses_dsl::buses
        .filter(buses_dsl::id.eq(booking.bus_id))
        .first::<Bus>(&mut conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    if let Some(bus) = bus {
        let new_count = (bus.current_passenger_count - 1).max(0);
        diesel::update(buses_dsl::buses.filter(buses_dsl::id.eq(booking.bus_id)))
            .set((
                buses_dsl::current_passenger_count.eq(new_count),
                buses_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
            ))
            .execute(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
    }

    let mut log = ActivityLog::new(ActivityType::BookingCancelled, current_user_id);
    log.set_target_id(booking_id);
    log.set_target_type("Booking".to_string());
    if let Ok(details) = serde_json::to_value(&updated_booking) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_booking)
}

pub async fn board_passenger(
    state: Arc<AppState>,
    booking_id: Uuid,
    admin_user_id: Uuid,
) -> Result<Booking, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booking = bookings_dsl::bookings
        .filter(bookings_dsl::id.eq(booking_id))
        .first::<Booking>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Booking not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if booking.status == BookingStatus::Boarded {
        return Err(ApiError::Error(
            "This booking has already been boarded".into(),
        ));
    }

    if booking.status == BookingStatus::Cancelled || booking.status == BookingStatus::Revoked {
        return Err(ApiError::Error(
            "This booking has already been cancelled or revoked".into(),
        ));
    }

    let updated_booking =
        diesel::update(bookings_dsl::bookings.filter(bookings_dsl::id.eq(booking_id)))
            .set((
                bookings_dsl::status.eq(BookingStatus::Boarded),
                bookings_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
            ))
            .get_result::<Booking>(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::BookingBoarded, admin_user_id);
    log.set_target_id(booking_id);
    log.set_target_type("Booking".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_booking)
}

pub async fn update_payment_status(
    state: Arc<AppState>,
    booking_id: Uuid,
    new_payment_status: PaymentStatus,
    admin_user_id: Uuid,
) -> Result<Booking, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_booking =
        diesel::update(bookings_dsl::bookings.filter(bookings_dsl::id.eq(booking_id)))
            .set((
                bookings_dsl::payment_status.eq(new_payment_status),
                bookings_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
            ))
            .get_result::<Booking>(&mut conn)
            .await
            .map_err(|e| match e {
                diesel::result::Error::NotFound => {
                    ApiError::NotFoundError("Booking not found".into())
                }
                _ => ApiError::InternalError(e.to_string().into()),
            })?;

    let mut log = ActivityLog::new(ActivityType::BookingPaymentUpdated, admin_user_id);
    log.set_target_id(booking_id);
    log.set_target_type("Booking".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_booking)
}

pub async fn get_booking_by_id(
    state: Arc<AppState>,
    booking_id: Uuid,
    current_user_id: Uuid,
    is_admin: bool,
) -> Result<Booking, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booking = bookings_dsl::bookings
        .filter(bookings_dsl::id.eq(booking_id))
        .first::<Booking>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Booking not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if !is_admin && booking.user_id != current_user_id {
        return Err(ApiError::PermissionDenied);
    }

    Ok(booking)
}

pub async fn get_my_bookings(
    state: Arc<AppState>,
    user_id: Uuid,
    params: GetBookingsQuery,
) -> Result<Vec<Booking>, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::bookings::table
        .filter(bookings_dsl::user_id.eq(user_id))
        .into_boxed::<diesel::pg::Pg>();

    if let Some(bus) = params.bus_id {
        query = query.filter(bookings_dsl::bus_id.eq(bus));
    }

    if let Some(route) = params.route_id {
        query = query.filter(bookings_dsl::route_id.eq(route));
    }

    if let Some(st) = params.status {
        query = query.filter(bookings_dsl::status.eq(st));
    }

    if let Some(ps) = params.payment_status {
        query = query.filter(bookings_dsl::payment_status.eq(ps));
    }

    query = query.order(bookings_dsl::created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Booking>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_bookings(
    state: Arc<AppState>,
    params: GetBookingsQuery,
) -> Result<Vec<Booking>, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::bookings::table.into_boxed::<diesel::pg::Pg>();

    if let Some(u) = params.user_id {
        query = query.filter(bookings_dsl::user_id.eq(u));
    }

    if let Some(bus) = params.bus_id {
        query = query.filter(bookings_dsl::bus_id.eq(bus));
    }

    if let Some(route) = params.route_id {
        query = query.filter(bookings_dsl::route_id.eq(route));
    }

    if let Some(st) = params.status {
        query = query.filter(bookings_dsl::status.eq(st));
    }

    if let Some(ps) = params.payment_status {
        query = query.filter(bookings_dsl::payment_status.eq(ps));
    }

    if let Some(ref r) = params.booking_ref {
        let r = r.trim();
        if !r.is_empty() {
            query = query.filter(bookings_dsl::booking_ref.ilike(format!("%{}%", r)));
        }
    }

    query = query.order(bookings_dsl::created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Booking>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_bus_seat_status(
    state: Arc<AppState>,
    bus_id: Uuid,
) -> Result<BusSeatsResponseDto, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;
    use crate::schema::buses::dsl as buses_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let bus = buses_dsl::buses
        .filter(buses_dsl::id.eq(bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // Load active booked seat numbers
    let active_bookings = bookings_dsl::bookings
        .filter(bookings_dsl::bus_id.eq(bus_id))
        .filter(bookings_dsl::status.ne(BookingStatus::Cancelled))
        .filter(bookings_dsl::status.ne(BookingStatus::Revoked))
        .select(bookings_dsl::seat_number)
        .load::<i32>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booked_set: HashSet<i32> = active_bookings.iter().cloned().collect();
    let mut available_seats = Vec::new();
    for seat in 1..=bus.capacity {
        if !booked_set.contains(&seat) {
            available_seats.push(seat);
        }
    }

    let mut booked_seats = active_bookings;
    booked_seats.sort_unstable();

    Ok(BusSeatsResponseDto {
        bus_id,
        capacity: bus.capacity,
        current_passenger_count: bus.current_passenger_count,
        booked_seats,
        available_seats,
    })
}

/// Return the stored QR code for a booking.
///
/// If the booking was created before the `qr_code` column existed (i.e. it's
/// NULL), generate it once, persist it, and return it.
pub async fn generate_booking_qr(
    state: Arc<AppState>,
    booking_id: Uuid,
    current_user_id: Uuid,
    is_admin: bool,
) -> Result<BookingQrCodeResponse, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booking = bookings_dsl::bookings
        .filter(bookings_dsl::id.eq(booking_id))
        .first::<Booking>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Booking not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // Non-admins can only access QR for their own bookings
    if !is_admin && booking.user_id != current_user_id {
        return Err(ApiError::PermissionDenied);
    }

    // Return the stored QR code, or lazily backfill if it's missing
    let qr_base64 = match booking.qr_code {
        Some(ref qr) => qr.clone(),
        None => {
            // Generate, persist, and return
            let qr_payload = build_signed_qr_payload(&booking)?;
            let generated = render_qr_to_base64(&qr_payload)?;

            diesel::update(bookings_dsl::bookings.filter(bookings_dsl::id.eq(booking_id)))
                .set(bookings_dsl::qr_code.eq(&generated))
                .execute(&mut conn)
                .await
                .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

            generated
        }
    };

    Ok(BookingQrCodeResponse {
        booking_id: booking.id,
        booking_ref: booking.booking_ref.clone(),
        qr_code_base64: qr_base64,
    })
}

/// Verify a scanned QR code payload. Admin-only.
///
/// Validates the HMAC signature, looks up the booking, and returns its
/// current status and payment information.
pub async fn verify_booking_qr(
    state: Arc<AppState>,
    payload: VerifyBookingDto,
) -> Result<BookingVerificationResult, ApiError> {
    use crate::schema::bookings::dsl as bookings_dsl;

    // Parse the QR payload JSON
    let qr_data: serde_json::Value = serde_json::from_str(&payload.qr_payload).map_err(|_| {
        ApiError::Error("Invalid QR code format".into())
    })?;

    let booking_id_str = qr_data
        .get("booking_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Error("QR code missing booking_id".into()))?;

    let booking_ref = qr_data
        .get("booking_ref")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Error("QR code missing booking_ref".into()))?;

    let user_id_str = qr_data
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Error("QR code missing user_id".into()))?;

    let provided_sig = qr_data
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::Error("QR code missing signature".into()))?;

    // Verify the HMAC signature
    let expected_sig = compute_hmac_signature(booking_id_str, booking_ref, user_id_str)?;
    if provided_sig != expected_sig {
        return Ok(BookingVerificationResult {
            valid: false,
            booking_id: None,
            booking_ref: None,
            status: None,
            payment_status: None,
            seat_number: None,
            message: "Invalid QR code: signature verification failed".to_string(),
        });
    }

    // Parse booking ID
    let booking_id: Uuid = booking_id_str.parse().map_err(|_| {
        ApiError::Error("Invalid booking_id in QR code".into())
    })?;

    // Look up the booking
    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let booking = bookings_dsl::bookings
        .filter(bookings_dsl::id.eq(booking_id))
        .first::<Booking>(&mut conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    match booking {
        Some(booking) => {
            let message = match booking.status {
                BookingStatus::Confirmed => "Booking is valid and confirmed.".to_string(),
                BookingStatus::Boarded => "Passenger has already boarded.".to_string(),
                BookingStatus::Cancelled => "Booking has been cancelled.".to_string(),
                BookingStatus::Revoked => "Booking has been revoked.".to_string(),
            };

            Ok(BookingVerificationResult {
                valid: booking.status == BookingStatus::Confirmed,
                booking_id: Some(booking.id),
                booking_ref: Some(booking.booking_ref),
                status: Some(booking.status),
                payment_status: Some(booking.payment_status),
                seat_number: Some(booking.seat_number),
                message,
            })
        }
        None => Ok(BookingVerificationResult {
            valid: false,
            booking_id: None,
            booking_ref: None,
            status: None,
            payment_status: None,
            seat_number: None,
            message: "Booking not found in the system.".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers for QR code generation and HMAC signing
// ---------------------------------------------------------------------------

/// Build a signed JSON payload to embed in the QR code.
fn build_signed_qr_payload(booking: &Booking) -> Result<String, ApiError> {
    let booking_id = booking.id.to_string();
    let user_id = booking.user_id.to_string();
    let signature = compute_hmac_signature(&booking_id, &booking.booking_ref, &user_id)?;

    let payload = serde_json::json!({
        "booking_id": booking_id,
        "booking_ref": booking.booking_ref,
        "user_id": user_id,
        "signature": signature,
    });

    serde_json::to_string(&payload)
        .map_err(|e| ApiError::InternalError(e.to_string().into()))
}

/// Compute HMAC-SHA256 over the canonical fields.
fn compute_hmac_signature(
    booking_id: &str,
    booking_ref: &str,
    user_id: &str,
) -> Result<String, ApiError> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let secret = std::env::var("QR_SIGNING_SECRET").unwrap_or_else(|_| {
        std::env::var("JWT_SECRET").expect("JWT_SECRET must be set")
    });

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| ApiError::InternalError(format!("HMAC error: {}", e).into()))?;

    mac.update(booking_id.as_bytes());
    mac.update(b"|");
    mac.update(booking_ref.as_bytes());
    mac.update(b"|");
    mac.update(user_id.as_bytes());

    let result = mac.finalize();
    Ok(hex::encode(result.into_bytes()))
}

/// Render a string payload as a QR code PNG and return it base64-encoded.
fn render_qr_to_base64(data: &str) -> Result<String, ApiError> {
    use base64::Engine;
    use image::ImageEncoder;
    use image::codecs::png::PngEncoder;
    use qrcode::QrCode;

    let code = QrCode::new(data.as_bytes())
        .map_err(|e| ApiError::InternalError(format!("QR generation error: {}", e).into()))?;

    // Render to a pixel buffer (each module = 8 pixels, with 4-module quiet zone)
    let image = code.render::<image::Luma<u8>>().quiet_zone(true).build();

    let mut png_bytes: Vec<u8> = Vec::new();
    let encoder = PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|e| ApiError::InternalError(format!("PNG encoding error: {}", e).into()))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
}
