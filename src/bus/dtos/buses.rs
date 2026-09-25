use super::super::models::buses::{Bus, BusState, BusStatus};
use super::*;
use crate::types::Pagination;
use string_validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Validate)]
pub struct CreateBusDto {
    pub license_plate: String,
    pub capacity: i32,
    pub state: Option<BusState>,
    pub status: Option<BusStatus>,
}

impl From<CreateBusDto> for Bus {
    fn from(dto: CreateBusDto) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: Uuid::now_v7(),
            license_plate: dto.license_plate,
            capacity: dto.capacity,
            current_passenger_count: 0,
            current_trip_id: None,
            state: dto.state.unwrap_or(BusState::Stationary),
            status: dto.status.unwrap_or(BusStatus::Available),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::buses)]
pub struct UpdateBusDto {
    pub license_plate: Option<String>,
    pub capacity: Option<i32>,
    pub current_passenger_count: Option<i32>,
    pub current_trip_id: Option<Option<Uuid>>,
    pub state: Option<BusState>,
    pub status: Option<BusStatus>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetBusesQuery {
    pub search: Option<String>,
    pub status: Option<BusStatus>,
    pub state: Option<BusState>,
    pub current_trip_id: Option<Uuid>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChangeBusStateDto {
    pub state: BusState,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChangeBusTripDto {
    pub trip_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AssignBusToTripDto {
    pub trip_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChangeBusStatusDto {
    pub status: BusStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChangeBusAllocationDto {
    pub passenger_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChangeBusCapacityDto {
    pub capacity: i32,
}
