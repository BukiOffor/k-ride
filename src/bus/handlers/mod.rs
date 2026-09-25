pub mod bookings;
pub mod buses;
pub mod points;
pub mod ride_request;
pub mod routes;
pub mod trips;

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(points::routes(state.clone()))
        .merge(buses::routes(state.clone()))
        .merge(routes::routes(state.clone()))
        .merge(trips::routes(state.clone()))
        .merge(bookings::routes(state.clone()))
        .merge(ride_request::routes(state))
}

pub use super::*;
