//! Services for Driver operations: availability, offers, active rides, lifecycle transitions, and location tracking.

use crate::AppState;
use crate::auth::jwt::Claims;
use crate::errors::ModuleError;
use crate::helpers::fares::calculate_waiting_charge;
use crate::schema::{driver_availability, ride_offers, rides, users};
use crate::vrp::dtos::rides::*;
use crate::vrp::models::rides::*;
use crate::vrp::realtime::{OutgoingWsMessage, WsEventFrame};
use crate::vrp::services::rides::{build_ride_projection, dispatch_ride_candidates};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;
use uuid::Uuid;

pub async fn get_driver_availability(
    app_state: Arc<AppState>,
    claims: Claims,
) -> Result<AvailabilityEnvelope, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let rec: Option<DriverAvailabilityRecord> = driver_availability::table
        .filter(driver_availability::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let availability = match rec {
        Some(r) => AvailabilityView {
            status: r.status(),
            last_location: r.lat.map(|lat| LiveLocation {
                point: LatLng {
                    lat,
                    lng: r.lng.unwrap_or_default(),
                },
                heading: r.heading,
                speed_mps: r.speed_mps,
                accuracy_meters: r.accuracy_m,
                recorded_at: r
                    .recorded_at
                    .map(|t| DateTime::from_naive_utc_and_offset(t, Utc))
                    .unwrap_or_else(Utc::now),
            }),
            updated_at: DateTime::from_naive_utc_and_offset(r.updated_at, Utc),
        },
        None => AvailabilityView {
            status: AvailabilityStatus::Offline,
            last_location: None,
            updated_at: Utc::now(),
        },
    };

    Ok(AvailabilityEnvelope { availability })
}

pub async fn set_driver_availability(
    app_state: Arc<AppState>,
    claims: Claims,
    payload: SetAvailabilityDto,
) -> Result<AvailabilityEnvelope, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // 1. Verify driver account is active
    let user: Option<crate::users::model::User> = users::table
        .filter(users::id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let user = user.ok_or(ModuleError::PermissionDenied)?;
    if payload.online && !user.is_active {
        return Err(ModuleError::NotAllowed(
            "Driver account is deactivated or suspended",
        ));
    }

    if payload.online {
        use crate::schema::{cars, driver_profiles};
        let approved = driver_profiles::table
            .inner_join(cars::table)
            .filter(driver_profiles::user_id.eq(claims.user_id))
            .filter(driver_profiles::status.eq("APPROVED"))
            .filter(cars::is_verified.eq(true))
            .select(driver_profiles::id)
            .first::<Uuid>(&mut conn)
            .await
            .optional()?
            .is_some();
        if !approved {
            return Err(ModuleError::NotAllowed(
                "Driver application and vehicle must be approved before going online",
            ));
        }
    }

    // 2. Check if on trip
    let live_trip: Option<Ride> = rides::table
        .filter(rides::driver_id.eq(claims.user_id))
        .filter(rides::status.eq_any(RideStatus::DRIVER_ATTACHED.iter().map(|s| s.to_string())))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if live_trip.is_some() && !payload.online {
        return Err(ModuleError::NotAllowed(
            "Cannot go offline while on a live trip",
        ));
    }

    let next_status = if payload.online {
        AvailabilityStatus::Online
    } else {
        AvailabilityStatus::Offline
    };

    // 3. If going offline, immediately remove from Redis spatial index
    if !payload.online {
        let mut d = (*app_state.dispatcher).clone();
        let _ = d.remove_driver(&claims.user_id.to_string()).await;
    }

    let record = DriverAvailabilityRecord {
        driver_id: claims.user_id,
        status: next_status.to_string(),
        lat: None,
        lng: None,
        heading: None,
        speed_mps: None,
        accuracy_m: None,
        recorded_at: None,
        received_at: Some(Utc::now().naive_utc()),
        updated_at: Utc::now().naive_utc(),
    };

    diesel::insert_into(driver_availability::table)
        .values(&record)
        .on_conflict(driver_availability::driver_id)
        .do_update()
        .set((
            driver_availability::status.eq(next_status.to_string()),
            driver_availability::updated_at.eq(Utc::now().naive_utc()),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let avail_view = AvailabilityView {
        status: next_status,
        last_location: None,
        updated_at: Utc::now(),
    };

    // 4. Push driver.availability.changed event down the WebSocket if connected
    let avail_evt = WsEventFrame {
        id: format!("evt_{}", Uuid::new_v4()),
        ride_id: None,
        seq: 1,
        ride_version: None,
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "driver.availability.changed",
            "availability": avail_view,
        }),
    };
    let res = app_state
        .ws_registry
        .send_to_driver(
            &claims.user_id,
            OutgoingWsMessage::Event { event: avail_evt },
        )
        .await;

    if let Err(e) = res {
        tracing::error!("Failed to send driver.availability.changed event: {}", e);
    };

    app_state
        .ws_registry
        .set_driver_online(claims.user_id, payload.online);

    Ok(AvailabilityEnvelope {
        availability: avail_view,
    })
}

pub async fn get_driver_current_ride(
    app_state: Arc<AppState>,
    claims: Claims,
) -> Result<RideEnvelope, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let live: Option<Ride> = rides::table
        .filter(rides::driver_id.eq(claims.user_id))
        .filter(rides::status.eq_any(RideStatus::DRIVER_ATTACHED.iter().map(|s| s.to_string())))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(r) = live {
        let proj = build_ride_projection(&mut conn, &r, claims.user_id, true).await?;
        return Ok(RideEnvelope { ride: Some(proj) });
    }

    // Recent finished trip window (15 mins)
    let cutoff = (Utc::now() - chrono::Duration::minutes(15)).naive_utc();
    let recent: Option<Ride> = rides::table
        .filter(rides::driver_id.eq(claims.user_id))
        .filter(rides::updated_at.ge(cutoff))
        .order(rides::updated_at.desc())
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    if let Some(r) = recent {
        let proj = build_ride_projection(&mut conn, &r, claims.user_id, true).await?;
        return Ok(RideEnvelope { ride: Some(proj) });
    }

    Ok(RideEnvelope { ride: None })
}

pub async fn accept_ride_offer(
    app_state: Arc<AppState>,
    claims: Claims,
    offer_id: Uuid,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let active: bool = users::table
        .filter(users::id.eq(claims.user_id))
        .select(users::is_active)
        .first(&mut conn)
        .await?;
    if !active {
        return Err(ModuleError::PermissionDenied);
    }

    let offer: RideOfferRecord = ride_offers::table
        .filter(ride_offers::id.eq(offer_id))
        .filter(ride_offers::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride offer not found".into()))?;

    if offer.status != "offered" {
        return Err(ModuleError::Error(
            "Offer already responded to or expired".into(),
        ));
    }

    if offer.expires_at < Utc::now().naive_utc() {
        let _ = diesel::update(ride_offers::table.filter(ride_offers::id.eq(offer_id)))
            .set(ride_offers::status.eq("expired"))
            .execute(&mut conn)
            .await;
        return Err(ModuleError::Error("Offer expired".into()));
    }

    // Atomic assignment compare-and-swap
    let updated_ride: Option<Ride> = diesel::update(
        rides::table
            .filter(rides::id.eq(offer.ride_id))
            .filter(diesel::dsl::exists(
                users::table
                    .filter(users::id.eq(claims.user_id))
                    .filter(users::is_active.eq(true)),
            ))
            .filter(rides::status.eq(RideStatus::Searching))
            .filter(rides::driver_id.is_null()),
    )
    .set((
        rides::status.eq(RideStatus::DriverAssigned),
        rides::driver_id.eq(claims.user_id),
        rides::version.eq(rides::version + 1),
        rides::assigned_at.eq(Utc::now().naive_utc()),
        rides::updated_at.eq(Utc::now().naive_utc()),
    ))
    .get_result(&mut conn)
    .await
    .optional()
    .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride = match updated_ride {
        Some(r) => {
            // Mark offer accepted
            let _ = diesel::update(ride_offers::table.filter(ride_offers::id.eq(offer_id)))
                .set((
                    ride_offers::status.eq("accepted"),
                    ride_offers::responded_at.eq(Utc::now().naive_utc()),
                ))
                .execute(&mut conn)
                .await;

            // Mark driver on trip
            let _ = diesel::update(
                driver_availability::table
                    .filter(driver_availability::driver_id.eq(claims.user_id)),
            )
            .set((
                driver_availability::status.eq(AvailabilityStatus::OnTrip.to_string()),
                driver_availability::updated_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut conn)
            .await;

            // Withdraw any other pending offers for this ride
            let other_offers: Vec<RideOfferRecord> = ride_offers::table
                .filter(ride_offers::ride_id.eq(offer.ride_id))
                .filter(ride_offers::id.ne(offer_id))
                .filter(ride_offers::status.eq("offered"))
                .load(&mut conn)
                .await
                .unwrap_or_default();

            for o in &other_offers {
                let withdrawn_evt = WsEventFrame {
                    id: format!("evt_{}", Uuid::now_v7()),
                    ride_id: Some(o.ride_id),
                    seq: 1,
                    ride_version: Some(r.version),
                    occurred_at: Utc::now().to_rfc3339(),
                    payload: serde_json::json!({
                        "type": "driver.request.withdrawn",
                        "offerId": o.id,
                        "reason": "taken"
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
                    .filter(ride_offers::ride_id.eq(offer.ride_id))
                    .filter(ride_offers::id.ne(offer_id))
                    .filter(ride_offers::status.eq("offered")),
            )
            .set(ride_offers::status.eq("withdrawn"))
            .execute(&mut conn)
            .await;

            r
        }
        None => {
            // Taken by someone else or cancelled
            let _ = diesel::update(ride_offers::table.filter(ride_offers::id.eq(offer_id)))
                .set(ride_offers::status.eq("withdrawn"))
                .execute(&mut conn)
                .await;
            return Err(ModuleError::Error(
                "Ride was already assigned or cancelled".into(),
            ));
        }
    };

    let proj = build_ride_projection(&mut conn, &ride, claims.user_id, true).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(ride.id),
        seq: 1,
        ride_version: Some(ride.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(ride.id, evt);
    app_state
        .ws_registry
        .set_driver_active_ride(claims.user_id, Some(ride.id));
    app_state
        .ws_registry
        .set_driver_online(claims.user_id, false);

    Ok(proj)
}

pub async fn decline_ride_offer(
    app_state: Arc<AppState>,
    claims: Claims,
    offer_id: Uuid,
) -> Result<(), ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let _ = diesel::update(
        ride_offers::table
            .filter(ride_offers::id.eq(offer_id))
            .filter(ride_offers::driver_id.eq(claims.user_id)),
    )
    .set((
        ride_offers::status.eq("declined"),
        ride_offers::responded_at.eq(Utc::now().naive_utc()),
    ))
    .execute(&mut conn)
    .await;

    Ok(())
}

pub async fn driver_arrived(
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
        .filter(rides::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if !ride.status.can_transition_to(RideStatus::DriverArrived) {
        return Err(ModuleError::NotAllowed(
            "Cannot transition to DRIVER_ARRIVED",
        ));
    }

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::DriverArrived),
            rides::version.eq(ride.version + 1),
            rides::arrived_at.eq(Utc::now().naive_utc()),
            rides::updated_at.eq(Utc::now().naive_utc()),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, true).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(updated.id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(updated.id, evt);

    Ok(proj)
}

pub async fn start_ride(
    app_state: Arc<AppState>,
    claims: Claims,
    ride_id: Uuid,
    payload: StartRideDto,
) -> Result<RideProjection, ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let ride: Ride = rides::table
        .filter(rides::id.eq(ride_id))
        .filter(rides::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if !ride.status.can_transition_to(RideStatus::InProgress) {
        return Err(ModuleError::NotAllowed(
            "Cannot start ride in its current state",
        ));
    }

    // Verify 4-digit PIN if provided
    if let Some(pin) = payload.pin {
        if pin.trim() != ride.trip_pin {
            return Err(ModuleError::BadRequest("Incorrect trip PIN"));
        }
    }

    let now = Utc::now().naive_utc();
    // Calculate waiting charge if driver marked arrived
    let opt: RideOption = serde_json::from_value(ride.option_snapshot.clone())?;
    let mut waiting_charge = 0;
    if let Some(arr) = ride.arrived_at {
        let (_, _, charge) = calculate_waiting_charge(&opt, arr, now);
        waiting_charge = charge;
    }

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::InProgress),
            rides::version.eq(ride.version + 1),
            rides::waiting_charge_minor.eq(waiting_charge),
            rides::started_at.eq(now),
            rides::updated_at.eq(now),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, true).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(updated.id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(updated.id, evt);

    Ok(proj)
}

pub async fn complete_ride(
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
        .filter(rides::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if !ride.status.can_transition_to(RideStatus::Completed) {
        return Err(ModuleError::NotAllowed(
            "Cannot complete ride in its current state",
        ));
    }

    let now = Utc::now().naive_utc();
    let final_fare = ride.fare_total_minor + ride.waiting_charge_minor;

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::Completed),
            rides::version.eq(ride.version + 1),
            rides::final_fare_minor.eq(Some(final_fare)),
            rides::payment_status.eq("paid"),
            rides::completed_at.eq(now),
            rides::updated_at.eq(now),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Free driver back to Online
    let _ = diesel::update(
        driver_availability::table.filter(driver_availability::driver_id.eq(claims.user_id)),
    )
    .set((
        driver_availability::status.eq(AvailabilityStatus::Online.to_string()),
        driver_availability::updated_at.eq(now),
    ))
    .execute(&mut conn)
    .await;

    app_state
        .ws_registry
        .set_driver_active_ride(claims.user_id, None);
    app_state
        .ws_registry
        .set_driver_online(claims.user_id, true);

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, true).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(updated.id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(updated.id, evt);

    Ok(proj)
}

pub async fn driver_cancel_ride(
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
        .filter(rides::driver_id.eq(claims.user_id))
        .first(&mut conn)
        .await
        .optional()
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?
        .ok_or(ModuleError::NotFoundError("Ride not found".into()))?;

    if ride.status == RideStatus::InProgress || ride.status.is_terminal() {
        return Err(ModuleError::NotAllowed(
            "Cannot cancel once ride has started",
        ));
    }

    // Driver cancelling before start resets ride to SEARCHING for re-dispatch
    let cancellation_data = CancellationView {
        by: "driver".into(),
        reason: payload.reason,
        fee: None,
    };

    let updated: Ride = diesel::update(rides::table.filter(rides::id.eq(ride_id)))
        .set((
            rides::status.eq(RideStatus::Searching),
            rides::driver_id.eq(None::<Uuid>),
            rides::version.eq(ride.version + 1),
            rides::cancellation.eq(serde_json::to_value(&cancellation_data)?),
            rides::updated_at.eq(Utc::now().naive_utc()),
        ))
        .get_result(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Set driver back online
    let _ = diesel::update(
        driver_availability::table.filter(driver_availability::driver_id.eq(claims.user_id)),
    )
    .set((
        driver_availability::status.eq(AvailabilityStatus::Online.to_string()),
        driver_availability::updated_at.eq(Utc::now().naive_utc()),
    ))
    .execute(&mut conn)
    .await;

    app_state
        .ws_registry
        .set_driver_active_ride(claims.user_id, None);
    app_state
        .ws_registry
        .set_driver_online(claims.user_id, true);

    // Trigger re-dispatch
    let bg_state = app_state.clone();
    tokio::spawn(async move {
        let _ = dispatch_ride_candidates(bg_state, ride_id).await;
    });

    let proj = build_ride_projection(&mut conn, &updated, claims.user_id, true).await?;

    let evt = WsEventFrame {
        id: format!("evt_{}", Uuid::now_v7()),
        ride_id: Some(updated.id),
        seq: 1,
        ride_version: Some(updated.version),
        occurred_at: Utc::now().to_rfc3339(),
        payload: serde_json::json!({
            "type": "ride.updated",
            "ride": proj,
        }),
    };
    app_state.ws_registry.broadcast_ride_event(updated.id, evt);

    Ok(proj)
}

pub async fn get_driver_history(
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
        .filter(rides::driver_id.eq(claims.user_id))
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

pub async fn update_driver_location_rest(
    app_state: Arc<AppState>,
    claims: Claims,
    location: LiveLocation,
) -> Result<(), ModuleError> {
    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    let now = Utc::now().naive_utc();
    diesel::insert_into(driver_availability::table)
        .values(&DriverAvailabilityRecord {
            driver_id: claims.user_id,
            status: AvailabilityStatus::Online.to_string(),
            lat: Some(location.point.lat),
            lng: Some(location.point.lng),
            heading: location.heading,
            speed_mps: location.speed_mps,
            accuracy_m: location.accuracy_meters,
            recorded_at: Some(location.recorded_at.naive_utc()),
            received_at: Some(now),
            updated_at: now,
        })
        .on_conflict(driver_availability::driver_id)
        .do_update()
        .set((
            driver_availability::lat.eq(location.point.lat),
            driver_availability::lng.eq(location.point.lng),
            driver_availability::heading.eq(location.heading),
            driver_availability::speed_mps.eq(location.speed_mps),
            driver_availability::accuracy_m.eq(location.accuracy_meters),
            driver_availability::recorded_at.eq(location.recorded_at.naive_utc()),
            driver_availability::received_at.eq(now),
            driver_availability::updated_at.eq(now),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))?;

    // Ingest into H3 dispatcher
    let mut dispatcher = (*app_state.dispatcher).clone();
    let state = dispatcher::DriverState {
        driver_id: claims.user_id.to_string(),
        lat: location.point.lat,
        lng: location.point.lng,
        available_seats: 4,
    };
    let _ = dispatcher.ingest_driver_location(&state).await;

    Ok(())
}
