use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::points::get_points,
        super::handlers::points::get_point_by_id,
        super::handlers::points::create_points,
        super::handlers::points::update_points,
        super::handlers::points::delete_points,
        super::handlers::buses::get_buses,
        super::handlers::buses::get_bus_by_id,
        super::handlers::buses::create_bus,
        super::handlers::buses::update_bus,
        super::handlers::buses::delete_bus,
        super::handlers::buses::reset_all_bus_routes,
        super::handlers::buses::change_bus_state,
        super::handlers::buses::change_bus_route,
        super::handlers::buses::change_bus_status,
        super::handlers::buses::reset_bus,
        super::handlers::buses::change_bus_allocation,
        super::handlers::buses::change_bus_capacity,
        super::handlers::buses::clear_bus_allocation,
    ),
    components(schemas(
        super::dtos::points::CreatePointDto,
        super::dtos::points::UpdatePointDto,
        super::dtos::points::GetPointsQuery,
        super::dtos::points::PointIdQuery,
        super::models::points::Point,
        super::models::points::GeoPoint,
        super::dtos::buses::CreateBusDto,
        super::dtos::buses::UpdateBusDto,
        super::dtos::buses::GetBusesQuery,
        super::dtos::buses::ChangeBusStateDto,
        super::dtos::buses::ChangeBusRouteDto,
        super::dtos::buses::ChangeBusStatusDto,
        super::dtos::buses::ChangeBusAllocationDto,
        super::dtos::buses::ChangeBusCapacityDto,
        super::models::buses::Bus,
        super::models::buses::BusState,
        super::models::buses::BusStatus,
    ))
)]
pub struct BusApiDoc;

pub type PointsApiDoc = BusApiDoc;
