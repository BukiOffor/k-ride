use super::super::models::cars::{Car, CarSize, CarTier, CarType};
use super::*;
use crate::types::Pagination;
use string_validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Validate)]
pub struct CreateCarDto {
    #[serde(alias = "plate", alias = "plate_number")]
    pub license_plate: String,
    #[serde(alias = "passenger_seats", alias = "seats")]
    pub capacity: i32,
    pub car_type: CarType,
    pub car_tier: CarTier,
    pub car_size: CarSize,
    #[serde(alias = "colour")]
    pub color: String,
    pub color_hex: Option<String>,
    #[serde(alias = "make")]
    pub brand: String,
    pub model: String,
    pub year: i32,
    pub vehicle_photo: Option<String>,
    pub vehicle_registration: Option<String>,
    pub insurance_certificate: Option<String>,
    pub roadworthiness_certificate: Option<String>,
    pub is_verified: Option<bool>,
}

impl From<CreateCarDto> for Car {
    fn from(dto: CreateCarDto) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: Uuid::now_v7(),
            license_plate: dto.license_plate,
            capacity: dto.capacity,
            car_type: dto.car_type,
            car_tier: dto.car_tier,
            car_size: dto.car_size,
            color: dto.color,
            color_hex: dto.color_hex,
            brand: dto.brand,
            model: dto.model,
            year: dto.year,
            vehicle_photo: dto.vehicle_photo,
            vehicle_registration: dto.vehicle_registration,
            insurance_certificate: dto.insurance_certificate,
            roadworthiness_certificate: dto.roadworthiness_certificate,
            is_verified: dto.is_verified.unwrap_or(false),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::cars)]
pub struct UpdateCarDto {
    pub license_plate: Option<String>,
    pub capacity: Option<i32>,
    pub car_type: Option<CarType>,
    pub car_tier: Option<CarTier>,
    pub car_size: Option<CarSize>,
    pub color: Option<String>,
    pub color_hex: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub year: Option<i32>,
    pub vehicle_photo: Option<String>,
    pub vehicle_registration: Option<String>,
    pub insurance_certificate: Option<String>,
    pub roadworthiness_certificate: Option<String>,
    pub is_verified: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetCarsQuery {
    pub search: Option<String>,
    pub car_type: Option<CarType>,
    pub car_tier: Option<CarTier>,
    pub car_size: Option<CarSize>,
    pub is_verified: Option<bool>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AssignCarToUserDto {
    pub user_id: Uuid,
}

pub type AssignCarDto = AssignCarToUserDto;
