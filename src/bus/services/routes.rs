use super::*;
use crate::bus::models::routes::Route;
use crate::logs::{ActivityLog, ActivityType};
use crate::traits::Validate;
use diesel::PgTextExpressionMethods;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_route(
    state: Arc<AppState>,
    payload: CreateRouteDto,
    user_id: Uuid,
) -> Result<Route, ApiError> {
    use crate::schema::routes::dsl::*;

    payload
        .validate()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    if payload.fare < 0.0 {
        return Err(ApiError::Error("Fare cannot be negative".into()));
    }
    if payload.distance < 0.0 {
        return Err(ApiError::Error("Distance cannot be negative".into()));
    }
    if payload.start_point == payload.end_point {
        return Err(ApiError::Error(
            "Start point and end point cannot be the same".into(),
        ));
    }

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let now = chrono::Utc::now().naive_utc();

    let new_route = Route {
        id: Uuid::now_v7(),
        name: payload.name.clone(),
        description: payload.description.clone(),
        start_point: payload.start_point,
        end_point: payload.end_point,
        fare: payload.fare,
        distance: payload.distance,
        stops: payload.stops.into_iter().map(Some).collect(),
        created_at: now,
        updated_at: now,
    };

    let inserted_route = diesel::insert_into(routes)
        .values(&new_route)
        .get_result::<Route>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::RouteCreated, user_id);
    log.set_target_id(inserted_route.id);
    log.set_target_type("Route".to_string());
    if let Ok(details) = serde_json::to_value(&inserted_route) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(inserted_route)
}

pub async fn get_routes(
    state: Arc<AppState>,
    params: GetRoutesQuery,
) -> Result<Vec<Route>, ApiError> {
    use crate::schema::routes::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::routes::table.into_boxed::<diesel::pg::Pg>();

    if let Some(ref s) = params.search {
        let s = s.trim();
        if !s.is_empty() {
            let words: Vec<&str> = s.split_whitespace().collect();
            for word in words {
                let pattern = format!("%{}%", word);
                query = query.filter(name.ilike(pattern.clone()).or(description.ilike(pattern)));
            }
        }
    }

    if let Some(sp) = params.start_point {
        query = query.filter(start_point.eq(sp));
    }

    if let Some(ep) = params.end_point {
        query = query.filter(end_point.eq(ep));
    }

    query = query.order(created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Route>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_route_by_id(state: Arc<AppState>, route_id: Uuid) -> Result<Route, ApiError> {
    use crate::schema::routes::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let route = routes
        .filter(id.eq(route_id))
        .first::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(route)
}

pub async fn update_route(
    state: Arc<AppState>,
    route_id: Uuid,
    payload: UpdateRouteDto,
    user_id: Uuid,
) -> Result<Route, ApiError> {
    use crate::schema::routes::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_route = diesel::update(routes.filter(id.eq(route_id)))
        .set((&payload, updated_at.eq(chrono::Utc::now().naive_utc())))
        .get_result::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::RouteUpdated, user_id);
    log.set_target_id(route_id);
    log.set_target_type("Route".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_route)
}

pub async fn delete_route(
    state: Arc<AppState>,
    route_id: Uuid,
    user_id: Uuid,
) -> Result<Route, ApiError> {
    use crate::schema::routes::dsl::*;

    let mut conn = state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let deleted_route = diesel::delete(routes.filter(id.eq(route_id)))
        .get_result::<Route>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Route not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::RouteDeleted, user_id);
    log.set_target_id(route_id);
    log.set_target_type("Route".to_string());
    if let Err(e) = state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted_route)
}
