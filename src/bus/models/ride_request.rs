use chrono::NaiveDate;

use super::*;

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Queryable,
    Identifiable,
    Selectable,
    Insertable,
    AsChangeset,
    utoipa::ToSchema,
)]
#[diesel(table_name = crate::schema::ride_requests)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RideRequest {
    pub id: Uuid,
    pub route_id: Uuid,
    pub count: i32,
    pub departure_date: NaiveDate,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
