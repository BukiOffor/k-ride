use crate::types::Pagination;
use super::super::models::routes::RideType;
use super::*;
use string_validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Validate)]
pub struct CreateRouteDto {
    pub name: String,
    pub description: String,
    pub start_point: Uuid,
    pub end_point: Uuid,
    pub fare: f32,
    pub distance: f32,
    pub ride_type: RideType,
    #[schema(value_type = Vec<String>)]
    pub stops: Vec<Uuid>,
    pub departure_time: chrono::NaiveTime,
    pub departure_date: chrono::NaiveDate,
    pub is_round_trip: Option<bool>,
    pub return_departure_time: Option<chrono::NaiveTime>,
    pub return_departure_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::routes)]
pub struct UpdateRouteDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub start_point: Option<Uuid>,
    pub end_point: Option<Uuid>,
    pub fare: Option<f32>,
    pub distance: Option<f32>,
    pub ride_type: Option<RideType>,
    pub stops: Option<Vec<Uuid>>,
    pub departure_time: Option<chrono::NaiveTime>,
    pub departure_date: Option<chrono::NaiveDate>,
    pub is_completed: Option<bool>,
    pub time_completed: Option<Option<NaiveDateTime>>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetRoutesQuery {
    pub search: Option<String>,
    pub start_point: Option<Uuid>,
    pub end_point: Option<Uuid>,
    pub ride_type: Option<RideType>,
    pub departure_date: Option<chrono::NaiveDate>,
    pub is_completed: Option<bool>,
    pub pagination: Option<Pagination>,
}
