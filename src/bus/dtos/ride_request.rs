use super::*;
use chrono::NaiveDate;
use crate::types::Pagination;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRideRequestDto {
    pub route_id: Uuid,
    pub departure_date: NaiveDate,
    /// Number of seats requested (defaults to 1 if not specified)
    pub count: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetRideRequestsQuery {
    pub route_id: Option<Uuid>,
    pub departure_date: Option<NaiveDate>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    pub pagination: Option<Pagination>,
}
