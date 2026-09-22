use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::points::get_points,
        super::handlers::points::create_points,
        super::handlers::points::update_points,
        super::handlers::points::delete_points,
    ),
    components(schemas(
        super::dtos::points::CreatePointDto,
        super::dtos::points::UpdatePointDto,
        super::dtos::points::GetPointsQuery,
        super::dtos::points::PointIdQuery,
        super::models::points::Point,
        super::models::points::GeoPoint,
    ))
)]
pub struct PointsApiDoc;
