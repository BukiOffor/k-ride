use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::cars::get_cars,
        super::handlers::cars::get_car_by_id,
        super::handlers::cars::create_car,
        super::handlers::cars::update_car,
        super::handlers::cars::delete_car,
        super::handlers::cars::assign_car_to_user,
        super::handlers::cars::unassign_car_from_user,
    ),
    components(schemas(
        super::dtos::cars::CreateCarDto,
        super::dtos::cars::UpdateCarDto,
        super::dtos::cars::GetCarsQuery,
        super::dtos::cars::AssignCarToUserDto,
        super::models::cars::Car,
        super::models::cars::CarType,
        super::models::cars::CarTier,
        super::models::cars::CarSize,
        super::dtos::drivers::DriverRegistrationDto,
        super::dtos::drivers::DriverProfileDto,
        super::dtos::drivers::LicenceClass,
        super::dtos::drivers::DriverTrack,
        super::dtos::drivers::PassengerSeats,
        super::models::drivers::DriverProfile,
    ))
)]
pub struct VrpApiDoc;

pub type CarsApiDoc = VrpApiDoc;
