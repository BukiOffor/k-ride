use super::*;
use crate::types::Pagination;
use string_validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Validate)]
pub struct CreateRouteDto {
    pub name: String,
    pub description: String,
    pub start_point: Uuid,
    pub end_point: Uuid,
    pub fare: f32,
    pub distance: f32,
    #[schema(value_type = Vec<String>)]
    pub stops: Vec<Uuid>,
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
    pub stops: Option<Vec<Option<Uuid>>>,
}

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetRoutesQuery {
    pub search: Option<String>,
    pub start_point: Option<Uuid>,
    pub end_point: Option<Uuid>,
    pub pagination: Option<Pagination>,
}
