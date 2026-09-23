use string_validator::Validate;

use super::super::models::points::{GeoPoint, Point};
use super::*;
use crate::types::Pagination;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Validate)]
pub struct CreatePointDto {
    pub name: String,
    pub geo_location: GeoPoint,
    pub landmark: String,
    pub description: String,
}

impl From<CreatePointDto> for Point {
    fn from(dto: CreatePointDto) -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: Uuid::now_v7(),
            name: dto.name,
            geo_location: dto.geo_location,
            landmark: dto.landmark,
            description: dto.description,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, AsChangeset, Default)]
#[diesel(table_name = crate::schema::points)]
pub struct UpdatePointPayload {
    pub name: Option<String>,
    pub geo_location: Option<GeoPoint>,
    pub landmark: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Default)]
pub struct UpdatePointDto {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub geo_location: Option<GeoPoint>,
    pub landmark: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetPointsQuery {
    pub id: Option<Uuid>,
    pub search: Option<String>,
    pub pagination: Option<Pagination>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct PointIdQuery {
    pub id: Option<Uuid>,
}

pub type DeletePointQuery = PointIdQuery;
