use crate::logs::{ActivityLog, ActivityType};
use crate::traits::Validate;

use super::*;
use diesel::PgTextExpressionMethods;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_bus(
    app_state: Arc<AppState>,
    payload: CreateBusDto,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    payload
        .validate()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let new_bus: Bus = payload.into();

    let bus = diesel::insert_into(buses)
        .values(&new_bus)
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::BusCreated, user_id);
    log.set_target_id(bus.id);
    log.set_target_type("Bus".to_string());
    if let Ok(details) = serde_json::to_value(&bus) {
        log.set_details(details);
    }
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(bus)
}

pub async fn get_buses(
    app_state: Arc<AppState>,
    params: GetBusesQuery,
) -> Result<Vec<Bus>, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::buses::table.into_boxed::<diesel::pg::Pg>();

    if let Some(ref s) = params.search {
        let s = s.trim();
        if !s.is_empty() {
            query = query.filter(license_plate.ilike(format!("%{}%", s)));
        }
    }

    if let Some(st) = params.status {
        query = query.filter(status.eq(st));
    }

    if let Some(bus_st) = params.state {
        query = query.filter(state.eq(bus_st));
    }

    if let Some(trip) = params.current_trip_id {
        query = query.filter(current_trip_id.eq(trip));
    }

    query = query.order(created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Bus>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_bus_by_id(app_state: Arc<AppState>, bus_id: Uuid) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let bus = buses
        .filter(id.eq(bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(bus)
}

pub async fn update_bus(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    payload: UpdateBusDto,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((&payload, updated_at.eq(chrono::Utc::now().naive_utc())))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusUpdated, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn delete_bus(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted_bus = diesel::delete(buses.filter(id.eq(bus_id)))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusDeleted, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted_bus)
}

pub async fn reset_all_bus_routes(
    app_state: Arc<AppState>,
    user_id: Uuid,
) -> Result<usize, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let count = diesel::update(buses)
        .set((
            current_passenger_count.eq(0),
            current_trip_id.eq(None::<Uuid>),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::BusRoutesReset, user_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "buses_reset_count": count
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(count)
}

pub async fn change_bus_state(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    new_state: BusState,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            state.eq(&new_state),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusStateChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "new_state": new_state
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn change_bus_trip(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    new_trip_id: Option<Uuid>,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate trip exists if provided
    if let Some(t_id) = new_trip_id {
        use crate::schema::trips::dsl as trips_dsl;
        trips_dsl::trips
            .filter(trips_dsl::id.eq(t_id))
            .first::<crate::bus::models::trips::Trip>(&mut conn)
            .await
            .map_err(|e| match e {
                diesel::result::Error::NotFound => {
                    ApiError::NotFoundError("Trip not found".into())
                }
                _ => ApiError::InternalError(e.to_string().into()),
            })?;
    }

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            current_trip_id.eq(new_trip_id),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusRouteChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "new_trip_id": new_trip_id
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn change_bus_status(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    new_status: BusStatus,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            status.eq(&new_status),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusStatusChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "new_status": new_status
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn reset_bus(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            current_passenger_count.eq(0),
            current_trip_id.eq(None::<Uuid>),
            state.eq(BusState::Stationary),
            status.eq(BusStatus::Available),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusReset, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn change_bus_allocation(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    passenger_count: i32,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    if passenger_count < 0 {
        return Err(ApiError::BadRequest("Passenger count cannot be negative"));
    }

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let bus = buses
        .filter(id.eq(bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if passenger_count > bus.capacity {
        return Err(ApiError::BadRequest(
            "Passenger count cannot exceed bus capacity",
        ));
    }

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            current_passenger_count.eq(passenger_count),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::BusAllocationChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "old_allocation": bus.current_passenger_count,
        "new_allocation": passenger_count
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn change_bus_capacity(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    new_capacity: i32,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    if new_capacity <= 0 {
        return Err(ApiError::BadRequest("Capacity must be greater than zero"));
    }

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let bus = buses
        .filter(id.eq(bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if new_capacity < bus.current_passenger_count {
        return Err(ApiError::BadRequest(
            "New capacity cannot be less than current passenger count",
        ));
    }

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            capacity.eq(new_capacity),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::BusCapacityChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "old_capacity": bus.capacity,
        "new_capacity": new_capacity
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn clear_bus_allocation(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            current_trip_id.eq(None::<Uuid>),
            current_passenger_count.eq(0),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::BusAllocationChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "cleared_allocation": true
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}

pub async fn assign_bus_to_trip(
    app_state: Arc<AppState>,
    bus_id: Uuid,
    trip_id: Uuid,
    user_id: Uuid,
) -> Result<Bus, ApiError> {
    use crate::schema::buses::dsl::*;
    use crate::schema::trips::dsl as trips_dsl;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // 1. Verify bus exists and is not under maintenance
    let bus = buses
        .filter(id.eq(bus_id))
        .first::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    if bus.status == BusStatus::Maintenance {
        return Err(ApiError::BadRequest(
            "Cannot assign trip to a bus currently under maintenance",
        ));
    }

    // 2. Verify trip exists
    trips_dsl::trips
        .filter(trips_dsl::id.eq(trip_id))
        .first::<crate::bus::models::trips::Trip>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Trip not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // 3. Update the bus with the trip assignment
    let updated_bus = diesel::update(buses.filter(id.eq(bus_id)))
        .set((
            current_trip_id.eq(Some(trip_id)),
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Bus>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Bus not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // 4. Also update the trip's bus_id field
    diesel::update(trips_dsl::trips.filter(trips_dsl::id.eq(trip_id)))
        .set((
            trips_dsl::bus_id.eq(Some(bus_id)),
            trips_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // 5. Log activity
    let mut log = ActivityLog::new(ActivityType::BusRouteChanged, user_id);
    log.set_target_id(bus_id);
    log.set_target_type("Bus".to_string());
    log.set_details(serde_json::json!({
        "trip_id": trip_id,
        "action": "assign_trip"
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_bus)
}
