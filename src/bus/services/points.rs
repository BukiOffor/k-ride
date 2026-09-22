use crate::traits::Validate;
use crate::logs::{ActivityLog, ActivityType};

use super::*;
use diesel::prelude::*;
use diesel::PgTextExpressionMethods;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_point(
    state: Arc<AppState>,
    payload: CreatePointDto,
    user_id: Uuid,
) -> Result<Point, ApiError> {
    use crate::schema::points::dsl::*;

    payload.validate().map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let new_point: Point = payload.into();

    let point = diesel::insert_into(points)
        .values(&new_point)
        .get_result::<Point>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::PointCreated, user_id);
    log.set_target_id(point.id);
    log.set_target_type("Point".to_string());
    if let Ok(details) = serde_json::to_value(&point) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(point)
}

pub async fn get_points(
    state: Arc<AppState>,
    params: GetPointsQuery,
) -> Result<Vec<Point>, ApiError> {
    use crate::schema::points::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::points::table.into_boxed::<diesel::pg::Pg>();

    if let Some(point_id) = params.id {
        query = query.filter(id.eq(point_id));
    }

    if let Some(ref s) = params.search {
        let s = s.trim();
        if !s.is_empty() {
            let words: Vec<&str> = s.split_whitespace().collect();
            for word in words {
                let pattern = format!("%{}%", word);
                query = query.filter(
                    name.ilike(pattern.clone())
                        .or(landmark.ilike(pattern.clone()))
                        .or(description.ilike(pattern)),
                );
            }
        }
    }

    query = query.order(created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Point>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn update_point(
    state: Arc<AppState>,
    point_id: Uuid,
    payload: UpdatePointPayload,
    user_id: Uuid,
) -> Result<Point, ApiError> {
    use crate::schema::points::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_point = diesel::update(points.filter(id.eq(point_id)))
        .set((
            &payload,
            updated_at.eq(chrono::Utc::now().naive_utc()),
        ))
        .get_result::<Point>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Point not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::PointUpdated, user_id);
    log.set_target_id(point_id);
    log.set_target_type("Point".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_point)
}

pub async fn delete_point(
    state: Arc<AppState>,
    point_id: Uuid,
    user_id: Uuid,
) -> Result<Point, ApiError> {
    use crate::schema::points::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted_point = diesel::delete(points.filter(id.eq(point_id)))
        .get_result::<Point>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Point not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::PointDeleted, user_id);
    log.set_target_id(point_id);
    log.set_target_type("Point".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted_point)
}

pub async fn get_point_by_id(
    state: Arc<AppState>,
    point_id: Uuid,
) -> Result<Point, ApiError> {
    use crate::schema::points::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let point = points
        .filter(id.eq(point_id))
        .first::<Point>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Point not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(point)
}
