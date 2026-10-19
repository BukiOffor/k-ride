//! Services for Paid Rides lifecycle, matching, projections, and state transitions.

use crate::auth::jwt::Claims;
use crate::errors::ModuleError;
use crate::helpers::fares::{calculate_fare_quote, calculate_waiting_charge, compute_route};
use crate::schema::{
    driver_availability, fare_estimates, idempotency_keys, ride_offers, ride_options, ride_ratings,
    rides, safety_events, users,
};
use crate::vrp::dtos::rides::*;
use crate::vrp::models::rides::*;
use crate::vrp::realtime::{OutgoingWsMessage, WsEventFrame};
use crate::{AppState, Connection};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use rand::Rng;
use std::sync::Arc;
use uuid::Uuid;

// =========================================================================================
// Idempotency Helpers
// =========================================================================================

pub async fn check_idempotency(
    conn: &mut Connection<'_>,
    user_id: Uuid,
    endpoint: &str,
    key: &str,
) -> Result<Option<(i32, serde_json::Value)>, ModuleError> {
    use crate::schema::idempotency_keys::dsl;

    let res: Option<IdempotencyRecord> = dsl::idempotency_keys
        .filter(dsl::user_id.eq(user_id))
        .filter(dsl::endpoint.eq(endpoint))
        .filter(dsl::idempotency_key.eq(key))
        .first(conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    Ok(res.map(|r| (r.status_code, r.response)))
}

pub async fn store_idempotency(
    conn: &mut Connection<'_>,
    user_id: Uuid,
    endpoint: &str,
    key: &str,
    status_code: i32,
    response: &serde_json::Value,
) -> Result<(), ModuleError> {
    let rec = IdempotencyRecord {
        user_id,
        endpoint: endpoint.to_string(),
        idempotency_key: key.to_string(),
        status_code,
        response: response.clone(),
        created_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(idempotency_keys::table)
        .values(&rec)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    Ok(())
}

// =========================================================================================
// Fare Estimation & Route Services
// =========================================================================================

pub async fn estimate_fare(
    app_state: Arc<AppState>,
    claims: Claims,
    payload: EstimateRequestDto,
) -> Result<FareEstimateDto, ModuleError> {
    if !payload.pickup.point.is_valid() || !payload.destination.point.is_valid() {
        return Err(ModuleError::BadRequest("Invalid coordinates supplied"));
    }

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // 1. Fetch active ride options
    let active_options: Vec<RideOption> = ride_options::table
        .filter(ride_options::is_active.eq(true))
        .order(ride_options::sort_order.asc())
        .load(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if active_options.is_empty() {
        return Err(ModuleError::Error(
            "No active ride products configured".into(),
        ));
    }

    // 2. Compute dynamic route using HTTP client
    let client = reqwest::Client::new();
    let route_res = compute_route(&client, &payload.pickup.point, &payload.destination.point).await;

    let route = Route {
        points: route_res.points,
        distance_meters: route_res.distance_meters,
        duration_seconds: route_res.duration_seconds,
        source: route_res.source,
    };

    // 3. For each product, calculate FareQuote and check candidate drivers nearby
    let mut estimate_options = Vec::new();

    // Query candidates nearby using dispatcher
    let mut dispatcher = (*app_state.dispatcher).clone();
    let candidates = dispatcher
        .query_candidates(payload.pickup.point.lat, payload.pickup.point.lng, 2)
        .await
        .unwrap_or_default();

    for opt in active_options {
        let fare_quote = calculate_fare_quote(&opt, route.distance_meters, route.duration_seconds);

        let available = !candidates.is_empty();
        let pickup_eta_seconds = if available { Some(300) } else { None };

        estimate_options.push(EstimateOption {
            option: RideOptionView::from(opt.clone()),
            fare: fare_quote,
            pickup_eta_seconds,
            available,
        });
    }

    let estimate_id = Uuid::now_v7();
    let expires_at = Utc::now() + chrono::Duration::minutes(15);

    let estimate_record = FareEstimateRecord {
        id: estimate_id,
        user_id: claims.user_id,
        pickup: serde_json::to_value(&payload.pickup)?,
        destination: serde_json::to_value(&payload.destination)?,
        route: serde_json::to_value(&route)?,
        options: serde_json::to_value(&estimate_options)?,
        expires_at: expires_at.naive_utc(),
        created_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(fare_estimates::table)
        .values(&estimate_record)
        .execute(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    Ok(FareEstimateDto {
        estimate_id,
        expires_at,
        route,
        options: estimate_options,
        payment_methods: vec![PaymentMethod::cash()],
        default_payment_method_id: "cash".into(),
    })
}

// =========================================================================================
// Ride Booking & Passenger Operations
// =========================================================================================

pub async fn request_ride(
    app_state: Arc<AppState>,
    claims: Claims,
    payload: RideRequestDto,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if crate::admin::requests_restricted(&mut conn, claims.user_id).await? {
        return Err(ModuleError::RequestsRestricted);
    }

    // Check if passenger already has a live ride
    let existing_live: Option<Ride> = rides::table
        .filter(rides::passenger_id.eq(claims.user_id))
        .filter(rides::status.eq_any(RideStatus::live_strings()))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(r) = existing_live {
        return build_ride_projection(&mut conn, &r, claims.user_id, false).await;
    }

    // Fetch and validate estimate
    let estimate: FareEstimateRecord = fare_estimates::table
        .filter(fare_estimates::id.eq(payload.estimate_id))
        .filter(fare_estimates::user_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Fare estimate not found".into()))?;

    if estimate.expires_at < Utc::now().naive_utc() {
        return Err(ModuleError::Error("Fare estimate expired".into()));
    }

    // Fetch RideOption
    let opt: RideOption = ride_options::table
        .filter(ride_options::id.eq(&payload.option_id))
        .first(&mut conn)
        .await
        .map_err(|_| ModuleError::NotFoundError("Selected ride option not found".into()))?;

    let estimate_options: Vec<EstimateOption> = serde_json::from_value(estimate.options)?;
    let selected_quote = estimate_options
        .into_iter()
        .find(|o| o.option.id == payload.option_id)
        .ok_or(ModuleError::BadRequest("Option not in original estimate"))?
        .fare;

    // Generate 4-digit pickup PIN
    let pin: u32 = rand::rng().random_range(1000..=9999);
    let trip_pin = format!("{:04}", pin);

    let ride_id = Uuid::now_v7();
    let new_ride = Ride {
        id: ride_id,
        passenger_id: claims.user_id,
        driver_id: None,
        status: RideStatus::Searching,
        version: 1,
        option_id: opt.id.clone(),
        option_snapshot: serde_json::to_value(&opt)?,
        estimate_id: estimate.id,
        pickup: estimate.pickup,
        destination: estimate.destination,
        route: estimate.route,
        fare: serde_json::to_value(&selected_quote)?,
        currency: opt.currency,
        fare_total_minor: selected_quote.total.amount_minor,
        waiting_charge_minor: 0,
        final_fare_minor: None,
        payment_method: "cash".into(),
        payment_status: "pending".into(),
        trip_pin,
        dispatch_round: 1,
        cancellation: None,
        tracking: None,
        rating: None,
        sos_status: "none".into(),
        created_at: Utc::now().naive_utc(),
        updated_at: Utc::now().naive_utc(),
        assigned_at: None,
        arrived_at: None,
        started_at: None,
        completed_at: None,
        cancelled_at: None,
    };

    let inserted_ride: Ride = diesel::insert_into(rides::table)
        .values(&new_ride)
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Trigger dispatch asynchronously
    let bg_state = app_state.clone();
    tokio::spawn(async move {
        let _ = dispatch_ride_candidates(bg_state, ride_id).await;
    });

    let proj = build_ride_projection(&mut conn, &inserted_ride, claims.user_id, false).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(ride_id),
        seq: 1,
        ride_version: Some(inserted_ride.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(ride_id, evt);

    Ok(proj)
}

pub async fn get_current_ride(
    app_state: Arc<AppState>,
    claims: Claims,
) -> Result<RideEnvelope, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Look for active live ride
    let live: Option<Ride> = rides::table
        .filter(rides::passenger_id.eq(claims.user_id))
        .filter(rides::status.eq_any(RideStatus::live_strings()))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(r) = live {
        let proj = build_ride_projection(&mut conn, &r, claims.user_id, false).await?;
        return Ok(RideEnvelope { ride: Some(proj) });
    }

    // Check for just-completed or just-cancelled ride within short window (15m / 2m)
    let recent_cutoff = (Utc::now() - chrono::Duration::minutes(15)).naive_utc();
    let recent: Option<Ride> = rides::table
        .filter(rides::passenger_id.eq(claims.user_id))
        .filter(rides::updated_at.ge(recent_cutoff))
        .order(rides::updated_at.desc())
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(r) = recent {
        let proj = build_ride_projection(&mut conn, &r, claims.user_id, false).await?;
        return Ok(RideEnvelope { ride: Some(proj) });
    }

    Ok(RideEnvelope { ride: None })
}

pub async fn get_ride_by_id(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id && ride.driver_id != Some(claims.user_id) {
        return Err(ModuleError::PermissionDenied);
    }

    let is_driver = ride.driver_id == Some(claims.user_id);
    build_ride_projection(&mut conn, &ride, claims.user_id, is_driver).await
}

pub async fn cancel_ride_passenger(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    payload: CancelRideDto,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id {
        return Err(ModuleError::PermissionDenied);
    }

    if !ride.status.can_transition_to(RideStatus::Cancelled) {
        return Err(ModuleError::NotAllowed(
            "Cannot cancel ride in its current state",
        ));
    }

    // In v1, cash only, no cancellation fees
    let cancellation_data = CancellationView {
        by: "passenger".into(),
        reason: payload.reason,
        fee: None,
    };

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::Cancelled),
            rides::version.eq(ride.version + 1),
            rides::cancellation.eq(serde_json::to_value(&cancellation_data)?),
            rides::cancelled_at.eq(Utc::now().naive_utc()),
            rides::updated_at.eq(Utc::now().naive_utc()),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Withdraw any offered offers
    let other_offers: Vec<RideOfferRecord> = ride_offers::table
        .filter(ride_offers::ride_id.eq(ride_id))
        .filter(ride_offers::status.eq("offered"))
        .load(&mut conn)
        .await
        .unwrap_or_default();

    for o in &other_offers {
        let withdrawn_evt = WsEventFrame {
            id: format!("evt_{}", Uuid::now_v7()),
            ride_id: Some(o.ride_id),
            seq: 1,
            ride_version: Some(updated.version),
            occurred_at: Utc::now().to_rfc3339(),
            payload: serde_json::json!({
                "type": "driver.request.withdrawn",
                "offerId": o.id,
                "reason": "cancelled"
            }),
        };
        let _ = app_state
            .ws_registry
            .send_to_driver(
                &o.driver_id,
                OutgoingWsMessage::Event {
                    event: withdrawn_evt,
                },
            )
            .await;
    }

    let _ = diesel::update(
        ride_offers::table
            .filter(ride_offers::ride_id.eq(ride_id))
            .filter(ride_offers::status.eq("offered")),
    )
    .set(ride_offers::status.eq("withdrawn"))
    .execute(&mut conn)
    .await;

    // Free driver if assigned
    if let Some(did) = updated.driver_id {
        let _ = diesel::update(
            driver_availability::table.filter(driver_availability::driver_id.eq(did)),
        )
        .set((
            driver_availability::status.eq(AvailabilityStatus::Online.to_string()),
            driver_availability::updated_at.eq(Utc::now().naive_utc()),
        ))
        .execute(&mut conn)
        .await;

        app_state.ws_registry.set_driver_active_ride(did, None);
        app_state.ws_registry.set_driver_online(did, true);
    }

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, false).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(ride_id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(ride_id, evt);

    Ok(proj)
}

pub async fn retry_ride_search(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    _payload: RetryRideDto,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id {
        return Err(ModuleError::PermissionDenied);
    }

    if ride.status != RideStatus::NoDrivers {
        return Err(ModuleError::NotAllowed(
            "Can only retry search when status is NO_DRIVERS",
        ));
    }

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::Searching),
            rides::version.eq(ride.version + 1),
            rides::dispatch_round.eq(ride.dispatch_round + 1),
            rides::updated_at.eq(Utc::now().naive_utc()),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let bg_state = app_state.clone();
    tokio::spawn(async move {
        let _ = dispatch_ride_candidates(bg_state, ride_id).await;
    });

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, false).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(ride_id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(ride_id, evt);

    Ok(proj)
}

pub async fn rate_ride(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    payload: RateRideDto,
) -> Result<RideProjection, ModuleError> {
    if !(1..=5).contains(&payload.stars) {
        return Err(ModuleError::BadRequest("Stars must be between 1 and 5"));
    }

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id && ride.driver_id != Some(claims.user_id) {
        return Err(ModuleError::PermissionDenied);
    }

    let is_passenger = ride.passenger_id == claims.user_id;
    let rater_role = if is_passenger { "passenger" } else { "driver" };
    let ratee_id = if is_passenger {
        ride.driver_id
            .ok_or(ModuleError::BadRequest("No driver to rate"))?
    } else {
        ride.passenger_id
    };

    let rating_record = RideRatingRecord {
        id: Uuid::now_v7(),
        ride_id,
        rater_id: claims.user_id,
        ratee_id,
        rater_role: rater_role.to_string(),
        stars: payload.stars,
        tags: payload.tags,
        comment: payload.comment,
        created_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(ride_ratings::table)
        .values(&rating_record)
        .execute(&mut conn)
        .await
        .map_err(|_| ModuleError::NotAllowed("Ride already rated by this participant"))?;

    let rating_view = RatingView {
        stars: payload.stars,
        submitted_at: Utc::now(),
    };

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::rating.eq(serde_json::to_value(&rating_view)?),
            rides::version.eq(ride.version + 1),
            rides::updated_at.eq(Utc::now().naive_utc()),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, !is_passenger).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(ride_id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(ride_id, evt);

    Ok(proj)
}

pub async fn get_passenger_history(
    app_state: Arc<AppState>,
    claims: Claims,
    _query: HistoryQuery,
) -> Result<RideHistoryPage, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride_list: Vec<Ride> = rides::table
        .filter(rides::passenger_id.eq(claims.user_id))
        .order(rides::created_at.desc())
        .limit(20)
        .load(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let mut items = Vec::new();
    for r in ride_list {
        let pickup: Place = serde_json::from_value(r.pickup)?;
        let destination: Place = serde_json::from_value(r.destination)?;
        let route: Route = serde_json::from_value(r.route)?;

        let final_total = r
            .final_fare_minor
            .unwrap_or(r.fare_total_minor + r.waiting_charge_minor);

        items.push(RideSummary {
            id: r.id,
            status: r.status,
            created_at: DateTime::from_naive_utc_and_offset(r.created_at, Utc),
            pickup_name: pickup.name,
            destination_name: destination.name,
            option_name: r.option_id,
            vehicle_class: "sedan".into(),
            total: Some(Money::new(final_total, &r.currency)),
            distance_meters: route.distance_meters,
            duration_seconds: Some(route.duration_seconds),
            counterpart_name: None,
            rating_given: None,
        });
    }

    Ok(RideHistoryPage {
        items,
        next_cursor: None,
    })
}

// =========================================================================================
// Viewer & Safety
// =========================================================================================

pub async fn get_viewer_profile(
    app_state: Arc<AppState>,
    claims: Claims,
) -> Result<ViewerDto, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let user: crate::users::model::User = users::table
        .filter(users::id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let mut roles = vec!["passenger".to_string()];
    if user.role == crate::users::RoleType::Driver {
        roles.push("driver".to_string());
    }

    Ok(ViewerDto {
        display_name: format!("{} {}", user.first_name, user.last_name),
        avatar_url: None,
        roles,
        vehicle: None,
    })
}

pub async fn report_sos(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    payload: SosDto,
) -> Result<AcceptedDto, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id && ride.driver_id != Some(claims.user_id) {
        return Err(ModuleError::PermissionDenied);
    }
    let role = if ride.passenger_id == claims.user_id {
        "passenger"
    } else {
        "driver"
    };

    let record = SafetyEventRecord {
        id: Uuid::now_v7(),
        ride_id,
        reporter_id: claims.user_id,
        reporter_role: role.into(),
        kind: "sos".into(),
        event_type: payload.sos_type.as_ref().into(),
        details: None,
        location: payload.location.and_then(|l| serde_json::to_value(l).ok()),
        driver_location: None,
        status: "sent".into(),
        occurred_at: payload
            .occurred_at
            .map(|t| t.naive_utc())
            .unwrap_or_else(|| Utc::now().naive_utc()),
        created_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(safety_events::table)
        .values(&record)
        .execute(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    app_state.ws_registry.broadcast_admin_event(
        "admin:safety",
        "incident.opened",
        serde_json::json!({"id":record.id,"rideId":ride_id}),
    );
    if record.kind == "sos" {
        app_state.ws_registry.broadcast_admin_event(
            "admin:safety",
            "sos.raised",
            serde_json::json!({"id":record.id,"rideId":ride_id}),
        );
    }
    Ok(AcceptedDto { accepted: true })
}

pub async fn report_safety_issue(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    payload: SafetyReportDto,
) -> Result<AcceptedDto, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.passenger_id != claims.user_id && ride.driver_id != Some(claims.user_id) {
        return Err(ModuleError::PermissionDenied);
    }
    let role = if ride.passenger_id == claims.user_id {
        "passenger"
    } else {
        "driver"
    };

    if ![
        "SAFETY_CONCERN",
        "DRIVER_CONDUCT",
        "PASSENGER_CONDUCT",
        "VEHICLE_ISSUE",
        "PAYMENT_DISPUTE",
        "OTHER",
    ]
    .contains(&payload.category.as_str())
    {
        return Err(ModuleError::BadRequest("Invalid safety report category"));
    }
    let record = SafetyEventRecord {
        id: Uuid::now_v7(),
        ride_id,
        reporter_id: claims.user_id,
        reporter_role: role.into(),
        kind: "report".into(),
        event_type: payload.category,
        details: payload.details,
        location: None,
        driver_location: None,
        status: "sent".into(),
        occurred_at: Utc::now().naive_utc(),
        created_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(safety_events::table)
        .values(&record)
        .execute(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    app_state.ws_registry.broadcast_admin_event(
        "admin:safety",
        "incident.opened",
        serde_json::json!({"id":record.id,"rideId":ride_id}),
    );
    if record.kind == "sos" {
        app_state.ws_registry.broadcast_admin_event(
            "admin:safety",
            "sos.raised",
            serde_json::json!({"id":record.id,"rideId":ride_id}),
        );
    }
    Ok(AcceptedDto { accepted: true })
}

pub async fn generate_share_link(
    app_state: Arc<AppState>,
    _claims: Claims,
    ride_id: Uuid,
) -> Result<ShareLinkDto, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let exists: bool = rides::table
        .filter(rides::id.eq(ride_id))
        .select(rides::id)
        .first::<Uuid>(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .is_some();

    if !exists {
        return Err(ModuleError::NotFoundError("Ride not found".into()));
    }

    let url = format!("https://app.k-ride.com/track/{}", ride_id);
    Ok(ShareLinkDto { url: Some(url) })
}

// =========================================================================================
// Dispatch Candidate Pipeline
// =========================================================================================

pub async fn dispatch_ride_candidates(
    app_state: Arc<AppState>,
    ride_id: Uuid,
) -> Result<(), ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = match rides::table
        .filter(rides::id.eq(ride_id))
        .first(&mut conn)
        .await
    {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    if ride.status != RideStatus::Searching {
        return Ok(());
    }

    let pickup: Place = serde_json::from_value(ride.pickup.clone())?;
    let destination: Place = serde_json::from_value(ride.destination.clone())?;
    let route: Route = serde_json::from_value(ride.route.clone())?;
    let fare_quote: FareQuote = serde_json::from_value(ride.fare.clone())?;
    let opt_view: RideOptionView = serde_json::from_value(ride.option_snapshot.clone())
        .map(|o: RideOption| RideOptionView::from(o))
        .unwrap_or_else(|_| RideOptionView {
            id: ride.option_id.clone(),
            name: "Ride".into(),
            description: "".into(),
            vehicle_class: "sedan".into(),
            capacity: 4,
            shared: false,
        });

    let passenger_user: Option<crate::users::model::User> = users::table
        .filter(users::id.eq(ride.passenger_id))
        .first(&mut conn)
        .await
        .optional()
        .unwrap_or(None);

    let passenger = RideOfferPassenger {
        display_name: passenger_user
            .map(|u| {
                format!("{} {}", u.first_name, u.last_name)
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| "Passenger".into()),
        rating: Some(5.0),
    };

    // Query online drivers using H3 spatial index within 5km radius
    let mut dispatcher = (*app_state.dispatcher).clone();
    let candidates = dispatcher
        .query_within_radius_routed(pickup.point.lat, pickup.point.lng, 5000.0, None)
        .await
        .unwrap_or_default();

    if candidates.is_empty() {
        // No drivers found
        let _ = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
            .set((
                rides::status.eq(RideStatus::NoDrivers),
                rides::version.eq(ride.version + 1),
                rides::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut conn)
            .await;

        if let Ok(updated_ride) = rides::table
            .filter(rides::id.eq(ride_id))
            .first(&mut conn)
            .await
        {
            if let Ok(proj) =
                build_ride_projection(&mut conn, &updated_ride, ride.passenger_id, false).await
            {
                let evt = WsEventFrame {
                    id: format!("evt_{}", Uuid::now_v7()),
                    ride_id: Some(ride_id),
                    seq: 1,
                    ride_version: Some(updated_ride.version),
                    occurred_at: Utc::now().to_rfc3339(),
                    payload: serde_json::json!({
                        "type": "ride.updated",
                        "ride": proj,
                    }),
                };
                app_state.ws_registry.broadcast_ride_event(ride_id, evt);
            }
        }
        return Ok(());
    }

    // Offer to candidate drivers
    let expires_at = Utc::now() + chrono::Duration::seconds(20);
    for candidate in candidates.iter().take(3) {
        if let Ok(driver_uuid) = Uuid::parse_str(&candidate.driver.driver_id) {
            // Must have active WebSocket connection in RAM
            if !app_state.ws_registry.is_driver_connected(&driver_uuid) {
                continue;
            }

            // Must be marked ONLINE in availability table
            let is_online: bool = driver_availability::table
                .filter(driver_availability::driver_id.eq(driver_uuid))
                .filter(driver_availability::status.eq("ONLINE"))
                .select(driver_availability::driver_id)
                .first::<Uuid>(&mut conn)
                .await
                .optional()
                .unwrap_or(None)
                .is_some();

            if !is_online {
                continue;
            }

            let offer_id = Uuid::now_v7();
            let pickup_dist_m = candidate.distance_m.round() as i64;
            let pickup_eta_s = candidate
                .duration_secs
                .map(|s| s.round() as i64)
                .unwrap_or(180);

            let offer = RideOfferRecord {
                id: offer_id,
                ride_id,
                driver_id: driver_uuid,
                dispatch_round: ride.dispatch_round,
                status: "offered".into(),
                pickup_distance_m: Some(pickup_dist_m as i32),
                pickup_eta_seconds: Some(pickup_eta_s as i32),
                expires_at: expires_at.naive_utc(),
                responded_at: None,
                created_at: Utc::now().naive_utc(),
            };

            let _ = diesel::insert_into(ride_offers::table)
                .values(&offer)
                .execute(&mut conn)
                .await;

            let offer_view = RideOfferView {
                offer_id,
                ride_id,
                expires_at,
                pickup: pickup.clone(),
                destination: destination.clone(),
                option: opt_view.clone(),
                fare: fare_quote.clone(),
                driver_earning: fare_quote.total.clone(),
                trip_distance_meters: route.distance_meters,
                trip_duration_seconds: route.duration_seconds,
                pickup_distance_meters: Some(pickup_dist_m),
                pickup_eta_seconds: Some(pickup_eta_s),
                passenger: passenger.clone(),
            };

            // Push down WebSocket to driver
            let event = WsEventFrame {
                id: format!("evt_{}", Uuid::now_v7()),
                ride_id: Some(ride_id),
                seq: 1,
                ride_version: Some(ride.version),
                occurred_at: Utc::now().to_rfc3339(),
                payload: serde_json::json!({
                    "type": "driver.request.received",
                    "offer": offer_view,
                }),
            };

            let _ = app_state
                .ws_registry
                .send_to_driver(&driver_uuid, OutgoingWsMessage::Event { event })
                .await;

            // Spawn background timer to expire offer if not responded
            let bg_state = app_state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
                if let Ok(mut bg_conn) = bg_state.pool.get().await {
                    let updated: Option<RideOfferRecord> = diesel::update(
                        ride_offers::table
                            .filter(ride_offers::id.eq(offer_id))
                            .filter(ride_offers::status.eq("offered")),
                    )
                    .set(ride_offers::status.eq("expired"))
                    .get_result(&mut bg_conn)
                    .await
                    .optional()
                    .unwrap_or(None);

                    if updated.is_some() {
                        let expired_event = WsEventFrame {
                            id: format!("evt_{}", Uuid::now_v7()),
                            ride_id: Some(ride_id),
                            seq: 1,
                            ride_version: None,
                            occurred_at: Utc::now().to_rfc3339(),
                            payload: serde_json::json!({
                                "type": "driver.request.expired",
                                "offerId": offer_id,
                            }),
                        };
                        let _ = bg_state
                            .ws_registry
                            .send_to_driver(
                                &driver_uuid,
                                OutgoingWsMessage::Event {
                                    event: expired_event,
                                },
                            )
                            .await;
                    }
                }
            });
        }
    }

    Ok(())
}

// =========================================================================================
// Projection Builder
// =========================================================================================

pub async fn build_ride_projection(
    conn: &mut Connection<'_>,
    ride: &Ride,
    current_user_id: Uuid,
    is_driver: bool,
) -> Result<RideProjection, ModuleError> {
    let pickup: Place = serde_json::from_value(ride.pickup.clone())?;
    let destination: Place = serde_json::from_value(ride.destination.clone())?;
    let route: Route = serde_json::from_value(ride.route.clone())?;
    let opt_view: RideOptionView = serde_json::from_value(ride.option_snapshot.clone())
        .map(|o: RideOption| RideOptionView::from(o))
        .unwrap_or_else(|_| RideOptionView {
            id: ride.option_id.clone(),
            name: "Ride".into(),
            description: "".into(),
            vehicle_class: "sedan".into(),
            capacity: 4,
            shared: false,
        });

    let mut fare_quote: FareQuote = serde_json::from_value(ride.fare.clone())?;
    let total_with_waiting = ride.fare_total_minor + ride.waiting_charge_minor;
    fare_quote.total = Money::new(
        ride.final_fare_minor.unwrap_or(total_with_waiting),
        &ride.currency,
    );

    // Waiting view calculation
    let opt_model: Option<RideOption> = serde_json::from_value(ride.option_snapshot.clone()).ok();
    let waiting = if let (Some(arr), Some(opt)) = (ride.arrived_at, opt_model) {
        let end_time = ride.started_at.unwrap_or_else(|| Utc::now().naive_utc());
        let (waited, billable_mins, charge) = calculate_waiting_charge(&opt, arr, end_time);
        let free_until = arr + chrono::Duration::minutes(opt.free_waiting_minutes as i64);

        Some(WaitingView {
            free_seconds: (opt.free_waiting_minutes as i64) * 60,
            per_minute: Money::new(opt.waiting_per_minute_minor, &opt.currency),
            free_until: Some(DateTime::from_naive_utc_and_offset(free_until, Utc)),
            waited_seconds: waited,
            billable_minutes: billable_mins,
            charge: Money::new(charge, &opt.currency),
        })
    } else {
        None
    };

    // Driver details
    let driver_view = if let Some(did) = ride.driver_id {
        let driver_user: Option<crate::users::model::User> = users::table
            .filter(users::id.eq(did))
            .first(conn)
            .await
            .optional()
            .ok()
            .flatten();

        driver_user.map(|u| DriverView {
            display_name: u.first_name,
            avatar_url: None,
            rating: Some(4.9),
            completed_rides: 150,
            verified: true,
            vehicle: Some(VehicleView {
                make: "Toyota".into(),
                model: "Corolla".into(),
                color: "Silver".into(),
                color_hex: Some("#C0C0C0".into()),
                plate: "ABC-123-XY".into(),
                vehicle_class: "sedan".into(),
                capacity: 4,
            }),
            contact: ContactView::none(),
        })
    } else {
        None
    };

    // Passenger details (present only in driver's projection)
    let passenger_view = if is_driver {
        let pass_user: Option<crate::users::model::User> = users::table
            .filter(users::id.eq(ride.passenger_id))
            .first(conn)
            .await
            .optional()
            .ok()
            .flatten();

        pass_user.map(|u| PassengerView {
            display_name: u.first_name,
            avatar_url: None,
            rating: Some(5.0),
            contact: ContactView::none(),
        })
    } else {
        None
    };

    // Trip PIN is visible to passenger only
    let trip_pin = if !is_driver && ride.passenger_id == current_user_id {
        Some(ride.trip_pin.clone())
    } else {
        None
    };

    let timestamps = TimestampsView {
        assigned_at: ride
            .assigned_at
            .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        arrived_at: ride
            .arrived_at
            .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        started_at: ride
            .started_at
            .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        completed_at: ride
            .completed_at
            .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
        cancelled_at: ride
            .cancelled_at
            .map(|t| DateTime::from_naive_utc_and_offset(t, Utc)),
    };

    let cancellation: Option<CancellationView> = ride
        .cancellation
        .as_ref()
        .and_then(|c| serde_json::from_value(c.clone()).ok());

    let rating: Option<RatingView> = ride
        .rating
        .as_ref()
        .and_then(|r| serde_json::from_value(r.clone()).ok());

    Ok(RideProjection {
        id: ride.id,
        version: ride.version,
        status: ride.status,
        created_at: DateTime::from_naive_utc_and_offset(ride.created_at, Utc),
        updated_at: DateTime::from_naive_utc_and_offset(ride.updated_at, Utc),
        pickup,
        destination,
        route,
        option: opt_view,
        fare: fare_quote,
        payment: PaymentView {
            status: ride.payment_status.clone(),
            method_label: "Cash".into(),
        },
        driver: driver_view,
        passenger: passenger_view,
        driver_earning: if is_driver {
            Some(Money::new(total_with_waiting, &ride.currency))
        } else {
            None
        },
        driver_location: None,
        approach: None,
        progress: None,
        waiting,
        cancellation,
        timestamps,
        sos: SosView {
            status: ride.sos_status.clone(),
        },
        rating,
        share_url: Some(format!("https://app.k-ride.com/track/{}", ride.id)),
        trip_pin,
    })
}
