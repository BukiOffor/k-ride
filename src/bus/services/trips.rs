use super::*;
use crate::bus::models::routes::Route;
use crate::bus::models::trips::{Trip, TripStatus};
use crate::logs::{ActivityLog, ActivityType};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_trip(
    state: Arc<AppState>,
    payload: CreateTripDto,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::routes::dsl as routes_dsl;
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate route exists
    routes_dsl::routes
        .filter(routes_dsl::id.eq(payload.route_id))
        .first::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if payload.arrival_time <= payload.departure_time {
        return Err(ApiError::Error(
            "Arrival time must be after departure time".into(),
        ));
    }

    let now = chrono::Utc::now().naive_utc();

    let new_trip = Trip {
        id: Uuid::now_v7(),
        route_id: payload.route_id,
        bus_id: payload.bus_id,
        driver_id: payload.driver_id,
        departure_time: payload.departure_time,
        arrival_time: payload.arrival_time,
        actual_departure_time: None,
        actual_arrival_time: None,
        ride_type: payload.ride_type,
        status: TripStatus::NotStarted,
        created_at: now,
        updated_at: now,
    };

    let inserted_trip = diesel::insert_into(trips)
        .values(&new_trip)
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    if let Some(target_bus_id) = payload.bus_id {
        use crate::schema::buses::dsl as buses_dsl;
        diesel::update(buses_dsl::buses.filter(buses_dsl::id.eq(target_bus_id)))
            .set((
                buses_dsl::current_trip_id.eq(Some(inserted_trip.id)),
                buses_dsl::updated_at.eq(now),
            ))
            .execute(&mut conn)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string().into()))?;
    }

    let mut log = ActivityLog::new(ActivityType::TripCreated, user_id);
    log.set_target_id(inserted_trip.id);
    log.set_target_type("Trip".to_string());
    if let Ok(details) = serde_json::to_value(&inserted_trip) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(inserted_trip)
}

pub async fn get_trips(
    state: Arc<AppState>,
    params: GetTripsQuery,
) -> Result<Vec<Trip>, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::trips::table.into_boxed::<diesel::pg::Pg>();

    if let Some(r) = params.route_id {
        query = query.filter(route_id.eq(r));
    }

    if let Some(b) = params.bus_id {
        query = query.filter(bus_id.eq(b));
    }

    if let Some(d) = params.driver_id {
        query = query.filter(driver_id.eq(d));
    }

    if let Some(s) = params.status {
        query = query.filter(status.eq(s));
    }

    if let Some(rt) = params.ride_type {
        query = query.filter(ride_type.eq(rt));
    }

    if let Some(from) = params.from_date {
        query = query.filter(departure_time.ge(from));
    }

    if let Some(to) = params.to_date {
        query = query.filter(departure_time.le(to));
    }

    query = query.order(departure_time.asc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Trip>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_trip_by_id(state: Arc<AppState>, trip_id: Uuid) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let trip = trips
        .filter(id.eq(trip_id))
        .first::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(trip)
}

pub async fn update_trip(
    state: Arc<AppState>,
    trip_id: Uuid,
    payload: UpdateTripDto,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_trip = diesel::update(trips.filter(id.eq(trip_id)))
        .set((&payload, updated_at.eq(chrono::Utc::now().naive_utc())))
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::TripUpdated, user_id);
    log.set_target_id(trip_id);
    log.set_target_type("Trip".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_trip)
}

pub async fn delete_trip(
    state: Arc<AppState>,
    trip_id: Uuid,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted_trip = diesel::delete(trips.filter(id.eq(trip_id)))
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::TripDeleted, user_id);
    log.set_target_id(trip_id);
    log.set_target_type("Trip".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted_trip)
}

pub async fn complete_trip(
    state: Arc<AppState>,
    trip_id: Uuid,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let now = chrono::Utc::now().naive_utc();

    let updated_trip = diesel::update(trips.filter(id.eq(trip_id)))
        .set((
            status.eq(TripStatus::Completed),
            actual_arrival_time.eq(Some(now)),
            updated_at.eq(now),
        ))
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::TripCompleted, user_id);
    log.set_target_id(trip_id);
    log.set_target_type("Trip".to_string());
    if let Ok(details) = serde_json::to_value(&updated_trip) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_trip)
}

pub async fn start_trip(
    state: Arc<AppState>,
    trip_id: Uuid,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Verify trip exists and is in the right state
    let trip = trips
        .filter(id.eq(trip_id))
        .first::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if trip.status != TripStatus::NotStarted {
        return Err(ApiError::Error(
            "Trip can only be started from NotStarted status".into(),
        ));
    }

    let now = chrono::Utc::now().naive_utc();

    let updated_trip = diesel::update(trips.filter(id.eq(trip_id)))
        .set((
            status.eq(TripStatus::InProgress),
            actual_departure_time.eq(Some(now)),
            updated_at.eq(now),
        ))
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::TripStarted, user_id);
    log.set_target_id(trip_id);
    log.set_target_type("Trip".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_trip)
}

pub async fn cancel_trip(
    state: Arc<AppState>,
    trip_id: Uuid,
    user_id: Uuid,
) -> Result<Trip, ApiError> {
    use crate::schema::trips::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let trip = trips
        .filter(id.eq(trip_id))
        .first::<Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if trip.status == TripStatus::Completed {
        return Err(ApiError::Error(
            "Cannot cancel a completed trip".into(),
        ));
    }

    let now = chrono::Utc::now().naive_utc();

    let updated_trip = diesel::update(trips.filter(id.eq(trip_id)))
        .set((
            status.eq(TripStatus::Cancelled),
            updated_at.eq(now),
        ))
        .get_result::<Trip>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::TripCancelled, user_id);
    log.set_target_id(trip_id);
    log.set_target_type("Trip".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_trip)
}
