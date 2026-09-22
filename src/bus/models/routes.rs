use super::*;

#[derive(
    Debug,
    Clone,
    Queryable,
    Identifiable,
    Selectable,
    Deserialize,
    Insertable,
    AsChangeset,
    utoipa::ToSchema,
)]
#[diesel(table_name = crate::schema::routes)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Route {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub start_point: Uuid,
    pub end_point: Uuid,
    pub fare: f32,
    pub distance: f32,
    pub stops: Vec<Uuid>,
    pub departure_time: chrono::NaiveTime,
    pub departure_date: chrono::NaiveDate,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
