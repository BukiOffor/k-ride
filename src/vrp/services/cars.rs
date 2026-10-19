use crate::logs::{ActivityLog, ActivityType};
use crate::traits::Validate;
use crate::users::model::User;

use super::*;
use diesel::PgTextExpressionMethods;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::sync::Arc;

pub async fn create_car(
    app_state: Arc<AppState>,
    payload: CreateCarDto,
    user_id: Uuid,
) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;

    payload
        .validate()
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let new_car: Car = payload.into();

    let car = diesel::insert_into(cars)
        .values(&new_car)
        .get_result::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => ApiError::Error("A car with this license plate already exists".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::CarCreated, user_id);
    log.set_target_id(car.id);
    log.set_target_type("Car".to_string());
    if let Ok(details) = serde_json::to_value(&car) {
        log.set_details(details);
    }
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(car)
}

pub async fn get_cars(
    app_state: Arc<AppState>,
    params: GetCarsQuery,
) -> Result<Vec<Car>, ApiError> {
    use crate::schema::cars::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut query = crate::schema::cars::table.into_boxed::<diesel::pg::Pg>();

    if let Some(ref s) = params.search {
        let s = s.trim();
        if !s.is_empty() {
            let pattern = format!("%{}%", s);
            query = query.filter(
                license_plate
                    .ilike(pattern.clone())
                    .or(brand.ilike(pattern.clone()))
                    .or(model.ilike(pattern)),
            );
        }
    }

    if let Some(ct) = params.car_type {
        query = query.filter(car_type.eq(ct));
    }

    if let Some(ct) = params.car_tier {
        query = query.filter(car_tier.eq(ct));
    }

    if let Some(cs) = params.car_size {
        query = query.filter(car_size.eq(cs));
    }

    if let Some(iv) = params.is_verified {
        query = query.filter(is_verified.eq(iv));
    }

    query = query.order(created_at.desc());

    if let Some(pagination) = params.pagination {
        let size = pagination.size.max(1) as i64;
        let page = pagination.page.max(1);
        let offset = ((page - 1) * pagination.size.max(1)) as i64;
        query = query.limit(size).offset(offset);
    }

    let result = query
        .load::<Car>(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    Ok(result)
}

pub async fn get_car_by_id(app_state: Arc<AppState>, car_id: Uuid) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let car = cars
        .filter(id.eq(car_id))
        .first::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Car not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    Ok(car)
}

pub async fn update_car(
    app_state: Arc<AppState>,
    car_id: Uuid,
    payload: UpdateCarDto,
    user_id: Uuid,
) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let updated_car = diesel::update(cars.filter(id.eq(car_id)))
        .set((&payload, updated_at.eq(chrono::Utc::now().naive_utc())))
        .get_result::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Car not found".into()),
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            ) => ApiError::Error("A car with this license plate already exists".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::CarUpdated, user_id);
    log.set_target_id(car_id);
    log.set_target_type("Car".to_string());
    if let Ok(details) = serde_json::to_value(&payload) {
        log.set_details(details);
    }
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(updated_car)
}

pub async fn delete_car(
    app_state: Arc<AppState>,
    car_id: Uuid,
    user_id: Uuid,
) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Unassign car from any users who currently have it assigned
    let _ = diesel::update(
        crate::schema::users::dsl::users.filter(crate::schema::users::dsl::car.eq(Some(car_id))),
    )
    .set((
        crate::schema::users::dsl::car.eq(None::<Uuid>),
        crate::schema::users::dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
    ))
    .execute(&mut conn)
    .await;

    let deleted_car = diesel::delete(cars.filter(id.eq(car_id)))
        .get_result::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Car not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let mut log = ActivityLog::new(ActivityType::CarDeleted, user_id);
    log.set_target_id(car_id);
    log.set_target_type("Car".to_string());
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(deleted_car)
}

pub async fn assign_car_to_user(
    app_state: Arc<AppState>,
    car_id: Uuid,
    target_user_id: Uuid,
    user_id: Uuid,
) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;
    use crate::schema::users::dsl as users_dsl;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate car exists
    let car = cars
        .filter(id.eq(car_id))
        .first::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Car not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    // Validate target user exists
    let _user = users_dsl::users
        .filter(users_dsl::id.eq(target_user_id))
        .first::<User>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("User not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let now = chrono::Utc::now().naive_utc();

    // If car is already assigned to another user, clear that assignment
    diesel::update(users_dsl::users.filter(users_dsl::car.eq(Some(car_id))))
        .set((
            users_dsl::car.eq(None::<Uuid>),
            users_dsl::updated_at.eq(now),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Assign car to target user
    diesel::update(users_dsl::users.filter(users_dsl::id.eq(target_user_id)))
        .set((
            users_dsl::car.eq(Some(car_id)),
            users_dsl::updated_at.eq(now),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::CarAssigned, user_id);
    log.set_target_id(car_id);
    log.set_target_type("Car".to_string());
    log.set_details(serde_json::json!({
        "car_id": car_id,
        "user_id": target_user_id,
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(car)
}

pub async fn unassign_car_from_user(
    app_state: Arc<AppState>,
    car_id: Uuid,
    user_id: Uuid,
) -> Result<Car, ApiError> {
    use crate::schema::cars::dsl::*;
    use crate::schema::users::dsl as users_dsl;

    let mut conn = app_state
        .pool
        .get()
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    // Validate car exists
    let car = cars
        .filter(id.eq(car_id))
        .first::<Car>(&mut conn)
        .await
        .map_err(|e| match e {
            diesel::result::Error::NotFound => ApiError::NotFoundError("Car not found".into()),
            _ => ApiError::InternalError(e.to_string().into()),
        })?;

    let now = chrono::Utc::now().naive_utc();

    diesel::update(users_dsl::users.filter(users_dsl::car.eq(Some(car_id))))
        .set((
            users_dsl::car.eq(None::<Uuid>),
            users_dsl::updated_at.eq(now),
        ))
        .execute(&mut conn)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string().into()))?;

    let mut log = ActivityLog::new(ActivityType::CarUnassigned, user_id);
    log.set_target_id(car_id);
    log.set_target_type("Car".to_string());
    log.set_details(serde_json::json!({
        "car_id": car_id,
    }));
    if let Err(e) = app_state.logger.try_send(log) {
        tracing::warn!("Failed to queue activity log: {}", e);
    }

    Ok(car)
}
