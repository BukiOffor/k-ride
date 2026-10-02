use super::super::models::routes::RideType;
use super::super::models::trips::TripStatus;
use super::*;
use crate::types::Pagination;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateTripDto {
    pub route_id: Uuid,
    pub bus_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub departure_time: NaiveDateTime,
    pub arrival_time: NaiveDateTime,
    pub ride_type: RideType,
    pub return_time: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::trips)]
pub struct UpdateTripDto {
    pub bus_id: Option<Option<Uuid>>,
    pub driver_id: Option<Option<Uuid>>,
    pub departure_time: Option<NaiveDateTime>,
    pub arrival_time: Option<NaiveDateTime>,
    pub ride_type: Option<RideType>,
    pub status: Option<TripStatus>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetTripsQuery {
    pub route_id: Option<Uuid>,
    pub bus_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub status: Option<TripStatus>,
    pub ride_type: Option<RideType>,
    pub from_date: Option<NaiveDateTime>,
    pub to_date: Option<NaiveDateTime>,
    pub pagination: Option<Pagination>,
}
