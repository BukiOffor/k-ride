use super::*;
use crate::bus::models::ride_request::RideRequest;
use crate::bus::models::routes::Route;
use crate::logs::{ActivityLog, ActivityType};
use diesel::OptionalExtension;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_or_increment_ride_request(
    state: Arc<AppState>,
    payload: CreateRideRequestDto,
    user_id: Uuid,
) -> Result<RideRequest, ApiError> {
    use crate::schema::ride_requests::dsl::*;
    use crate::schema::routes::dsl as routes_dsl;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate that the route exists
    routes_dsl::routes
        .filter(routes_dsl::id.eq(payload.route_id))
        .first::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let request_count = payload.count.unwrap_or(1).max(1);

    // Check if an existing ride request already exists for this route and departure date
    let existing = ride_requests
        .filter(route_id.eq(payload.route_id))
        .filter(departure_date.eq(payload.departure_date))
        .first::<RideRequest>(&mut conn)
        .await
        .optional()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let now = chrono::Utc::now().naive_utc();

    let result = match existing {
        Some(existing_req) => {
            diesel::update(ride_requests.filter(id.eq(existing_req.id)))
                .set((
                    count.eq(existing_req.count + request_count),
                    updated_at.eq(now),
                ))
                .get_result::<RideRequest>(&mut conn)
                .await
                .map_err(|e| ApiError::InternalError(e.to_string().into()))?
        }
        None => {
            let new_request = RideRequest {
                id: Uuid::now_v7(),
                route_id: payload.route_id,
                count: request_count,
                departure_date: payload.departure_date,
                created_at: now,
                updated_at: now,
            };

            diesel::insert_into(ride_requests)
                .values(&new_request)
                .get_result::<RideRequest>(&mut conn)
                .await
                .map_err(|e| ApiError::InternalError(e.to_string().into()))?
        }
    };

    let mut log = ActivityLog::new(ActivityType::RideRequestCreated, user_id);
    log.set_target_id(result.id);
    log.set_target_type("RideRequest".to_string());
    if let Ok(details) = serde_json::to_value(&result) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(result)
}

pub async fn get_ride_requests(
    state: Arc<AppState>,
    params: GetRideRequestsQuery,
) -> Result<Vec<RideRequest>, ApiError> {
    use crate::schema::ride_requests::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::ride_requests::table.into_boxed::<diesel::pg::Pg>();

    if let Some(r) = params.route_id {
        query = query.filter(route_id.eq(r));
    }

    if let Some(d) = params.departure_date {
        query = query.filter(departure_date.eq(d));
    }

    if let Some(from) = params.from_date {
        query = query.filter(departure_date.ge(from));
    }

    if let Some(to) = params.to_date {
        query = query.filter(departure_date.le(to));
    }

    query = query.order(departure_date.asc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<RideRequest>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_ride_request_by_id(
    state: Arc<AppState>,
    request_id: Uuid,
) -> Result<RideRequest, ApiError> {
    use crate::schema::ride_requests::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let req = ride_requests
        .filter(id.eq(request_id))
        .first::<RideRequest>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => {
                ApiError::NotFoundError("Ride request not found".into())
            }
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(req)
}

pub async fn delete_ride_request(
    state: Arc<AppState>,
    request_id: Uuid,
    user_id: Uuid,
) -> Result<RideRequest, ApiError> {
    use crate::schema::ride_requests::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted = diesel::delete(ride_requests.filter(id.eq(request_id)))
        .get_result::<RideRequest>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => {
                ApiError::NotFoundError("Ride request not found".into())
            }
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::RideRequestDeleted, user_id);
    log.set_target_id(request_id);
    log.set_target_type("RideRequest".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted)
}
