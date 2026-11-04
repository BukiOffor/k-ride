//! Rental requests fulfilled offline by administrators.
use crate::schema::rental_requests::{self, dsl::*};
use crate::vrp::CarType;
use crate::{AppState, ModuleError, auth::jwt::Claims, users::RoleType};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Serialize, Queryable, Selectable, utoipa::ToSchema)]
#[diesel(table_name = rental_requests)]
#[serde(rename_all = "camelCase")]
pub struct RentalRequest {
    pub id: Uuid,
    pub user_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub pickup_location: String,
    pub notes: String,
    pub status: String,
    pub assigned_by: Option<Uuid>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub price: Option<i64>,
    pub car_type: Option<CarType>,
    pub driver_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RentalInput {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub pickup_location: String,
    pub car_type: CarType,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignRentalInput {
    pub driver_id: Uuid,
}

impl RentalInput {
    fn validate(&self, now: DateTime<Utc>) -> Result<(), ModuleError> {
        if self.starts_at <= now {
            return Err(ModuleError::BadRequest("Hire must start in the future"));
        }
        if self.ends_at.signed_duration_since(self.starts_at) < chrono::Duration::hours(24) {
            return Err(ModuleError::BadRequest("Minimum hire duration is 24 hours"));
        }
        if self.pickup_location.trim().is_empty()
            || self.pickup_location.chars().count() > 1000
            || self.notes.chars().count() > 5000
        {
            return Err(ModuleError::BadRequest(
                "Pickup location is required (maximum 1000 characters); notes maximum 5000 characters",
            ));
        }
        Ok(())
    }
}
#[derive(Default, Deserialize, utoipa::IntoParams)]
#[serde(deny_unknown_fields)]
pub struct RentalQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
fn admin(c: &Claims) -> Result<(), ModuleError> {
    if matches!(c.role, RoleType::Admin | RoleType::Root) {
        Ok(())
    } else {
        Err(ModuleError::PermissionDenied)
    }
}

pub fn routes(s: Arc<AppState>) -> Router {
    Router::new()
        .route("/rentals", get(list).post(create))
        .route("/rentals/{id}", get(detail).put(update).delete(remove))
        .route("/admin/rentals", get(admin_list))
        .route("/admin/rentals/{id}", get(admin_detail))
        .route("/admin/rentals/{id}/assign", post(assign))
        .route("/rental/pricing", get(pricing))
        .with_state(s)
}

#[utoipa::path(post, path="/rentals", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), request_body=RentalInput, responses((status=201,body=RentalRequest),(status=400,description="Invalid rental")))]
pub async fn create(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<RentalInput>,
) -> Result<(StatusCode, Json<RentalRequest>), ModuleError> {
    p.validate(Utc::now())?;
    let mut conn = s.pool.get().await?;
    let car_type_str = match p.car_type {
        CarType::Van => "van",
        CarType::SUV => "suv",
        CarType::Saloon => "saloon",
    };
    let days = p.ends_at.signed_duration_since(p.starts_at).num_days();
    let rental_price = get_pricing()
        .get("cars")
        .unwrap()
        .get(car_type_str)
        .unwrap()
        .get("price")
        .unwrap()
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap()
        * days;
    let row = diesel::insert_into(rental_requests::table)
        .values((
            id.eq(Uuid::now_v7()),
            user_id.eq(c.user_id),
            starts_at.eq(p.starts_at),
            ends_at.eq(p.ends_at),
            pickup_location.eq(p.pickup_location.trim()),
            notes.eq(p.notes),
            price.eq(rental_price),
            car_type.eq(p.car_type),
        ))
        .get_result(&mut conn)
        .await?;
    Ok((StatusCode::CREATED, Json(row)))
}
async fn rows(
    s: Arc<AppState>,
    owner: Option<Uuid>,
    q: RentalQuery,
) -> Result<Json<Vec<RentalRequest>>, ModuleError> {
    if q.status
        .as_ref()
        .is_some_and(|v| v != "pending" && v != "assigned")
        || q.limit.is_some_and(|v| !(1..=100).contains(&v))
        || q.offset.is_some_and(|v| v < 0)
    {
        return Err(ModuleError::BadRequest("Invalid status or pagination"));
    }
    let mut query = rental_requests::table.into_boxed();
    if let Some(uid) = owner {
        query = query.filter(user_id.eq(uid));
    }
    if let Some(value) = q.status {
        query = query.filter(status.eq(value));
    }
    let mut conn = s.pool.get().await?;
    Ok(Json(
        query
            .order((created_at.desc(), id.desc()))
            .limit(q.limit.unwrap_or(50))
            .offset(q.offset.unwrap_or(0))
            .load(&mut conn)
            .await?,
    ))
}
#[utoipa::path(get, path="/rentals", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(RentalQuery), responses((status=200,body=Vec<RentalRequest>)))]
pub async fn list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(q): Query<RentalQuery>,
) -> Result<Json<Vec<RentalRequest>>, ModuleError> {
    rows(s, Some(c.user_id), q).await
}
#[utoipa::path(get, path="/admin/rentals", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(RentalQuery), responses((status=200,body=Vec<RentalRequest>),(status=403,description="Admin required")))]
pub async fn admin_list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(q): Query<RentalQuery>,
) -> Result<Json<Vec<RentalRequest>>, ModuleError> {
    admin(&c)?;
    rows(s, None, q).await
}

#[utoipa::path(get, path="/rentals/{id}", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(("id"=Uuid,Path)), responses((status=200,body=RentalRequest),(status=404,description="Not found")))]
pub async fn detail(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(key): Path<Uuid>,
) -> Result<Json<RentalRequest>, ModuleError> {
    let mut conn = s.pool.get().await?;
    Ok(Json(
        rental_requests::table
            .filter(id.eq(key))
            .filter(user_id.eq(c.user_id))
            .first(&mut conn)
            .await?,
    ))
}
#[utoipa::path(get, path="/admin/rentals/{id}", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(("id"=Uuid,Path)), responses((status=200,body=RentalRequest),(status=403,description="Admin required"),(status=404,description="Not found")))]
pub async fn admin_detail(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(key): Path<Uuid>,
) -> Result<Json<RentalRequest>, ModuleError> {
    admin(&c)?;
    let mut conn = s.pool.get().await?;
    Ok(Json(
        rental_requests::table.find(key).first(&mut conn).await?,
    ))
}

// Conditional mutations prevent edits/deletion racing with assignment. An owner-only
// existence check distinguishes a closed request from a missing or another user's request.
async fn mutation_error(
    conn: &mut diesel_async::AsyncPgConnection,
    key: Uuid,
    owner: Uuid,
) -> ModuleError {
    match rental_requests::table
        .filter(id.eq(key))
        .filter(user_id.eq(owner))
        .select(id)
        .first::<Uuid>(conn)
        .await
    {
        Ok(_) => ModuleError::Conflict("Assigned rentals cannot be changed or deleted"),
        Err(e) => e.into(),
    }
}
#[utoipa::path(put, path="/rentals/{id}", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(("id"=Uuid,Path)), request_body=RentalInput, responses((status=200,body=RentalRequest),(status=400,description="Invalid rental"),(status=404,description="Not found"),(status=409,description="Already assigned")))]
pub async fn update(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(key): Path<Uuid>,
    Json(p): Json<RentalInput>,
) -> Result<Json<RentalRequest>, ModuleError> {
    p.validate(Utc::now())?;
    let mut conn = s.pool.get().await?;
    let result = diesel::update(
        rental_requests::table
            .filter(id.eq(key))
            .filter(user_id.eq(c.user_id))
            .filter(status.eq("pending")),
    )
    .set((
        starts_at.eq(p.starts_at),
        ends_at.eq(p.ends_at),
        pickup_location.eq(p.pickup_location.trim()),
        notes.eq(p.notes),
        updated_at.eq(Utc::now()),
    ))
    .get_result::<RentalRequest>(&mut conn)
    .await
    .optional()?;
    match result {
        Some(row) => Ok(Json(row)),
        None => Err(mutation_error(&mut conn, key, c.user_id).await),
    }
}
#[utoipa::path(delete, path="/rentals/{id}", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(("id"=Uuid,Path)), responses((status=204,description="Deleted"),(status=404,description="Not found"),(status=409,description="Already assigned")))]
pub async fn remove(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(key): Path<Uuid>,
) -> Result<StatusCode, ModuleError> {
    let mut conn = s.pool.get().await?;
    let count = diesel::delete(
        rental_requests::table
            .filter(id.eq(key))
            .filter(user_id.eq(c.user_id))
            .filter(status.eq("pending")),
    )
    .execute(&mut conn)
    .await?;
    if count == 0 {
        return Err(mutation_error(&mut conn, key, c.user_id).await);
    }
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post, path="/admin/rentals/{id}/assign", tag="Rentals", security(("bearerAuth"=[]),("cookieAuth"=[])), params(("id"=Uuid,Path)), responses((status=200,body=RentalRequest),(status=403,description="Admin required"),(status=404,description="Not found"),(status=409,description="Hire has already started")))]
pub async fn assign(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(key): Path<Uuid>,
    Json(p): Json<AssignRentalInput>,
) -> Result<Json<RentalRequest>, ModuleError> {
    admin(&c)?;
    let mut conn = s.pool.get().await?;
    let now = Utc::now();
    let result = diesel::update(
        rental_requests::table
            .filter(id.eq(key))
            .filter(status.eq("pending"))
            .filter(starts_at.gt(now))
            .filter(driver_id.eq(p.driver_id)),
    )
    .set((
        status.eq("assigned"),
        assigned_by.eq(c.user_id),
        assigned_at.eq(now),
        updated_at.eq(now),
    ))
    .get_result::<RentalRequest>(&mut conn)
    .await
    .optional()?;
    if let Some(row) = result {
        return Ok(Json(row));
    }
    let row: RentalRequest = rental_requests::table.find(key).first(&mut conn).await?;
    if row.status == "assigned" {
        return Ok(Json(row));
    }
    Err(ModuleError::Conflict("Hire has already started"))
}


pub async fn pricing(
    State(_s): State<Arc<AppState>>,
    Json(_p): Json<RentalInput>,
) -> Result<Json<Value>, ModuleError> {
    Ok(Json(get_pricing()))
}

fn get_pricing() -> Value {
    json!({
        "cars": {
            "van": {
                "price": "100000",
                "currency": "NGN"
            },
            "suv": {
                "price": "200000",
                "currency": "NGN"
            },
            "saloon": {
                "price": "300000",
                "currency": "NGN"
            }
        }
    })
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(create, list, detail, update, remove, admin_list, admin_detail, assign),
    components(schemas(RentalRequest, RentalInput))
)]
pub struct RentalApiDoc;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enforces_duration_and_input_limits() {
        let now = Utc::now();
        let mut p = RentalInput {
            starts_at: now + chrono::Duration::hours(1),
            ends_at: now + chrono::Duration::hours(25),
            pickup_location: "Airport".into(),
            car_type: CarType::Saloon,
            notes: String::new(),
        };
        assert!(p.validate(now).is_ok());
        p.ends_at -= chrono::Duration::seconds(1);
        assert!(p.validate(now).is_err());
        p.ends_at += chrono::Duration::seconds(1);
        p.pickup_location = "  ".into();
        assert!(p.validate(now).is_err());
        p.pickup_location = "Airport".into();
        p.starts_at = now;
        assert!(p.validate(now).is_err());
    }
    #[test]
    fn ordinary_roles_cannot_assign() {
        for role in [RoleType::User, RoleType::Driver, RoleType::RouteCoordinator] {
            assert!(
                admin(&Claims {
                    user_id: Uuid::now_v7(),
                    role,
                    exp: 0
                })
                .is_err()
            );
        }
    }
    #[test]
    fn clients_cannot_set_status_or_ownership() {
        assert!(
            serde_json::from_value::<RentalInput>(serde_json::json!({
                "startsAt": "2026-12-01T09:00:00Z", "endsAt": "2026-12-02T09:00:00Z",
                "pickupLocation": "Airport", "status": "assigned"
            }))
            .is_err()
        );
    }
    #[test]
    fn only_admin_and_root_can_assign() {
        for role in [RoleType::Admin, RoleType::Root] {
            assert!(
                admin(&Claims {
                    user_id: Uuid::now_v7(),
                    role,
                    exp: 0
                })
                .is_ok()
            );
        }
    }
}
