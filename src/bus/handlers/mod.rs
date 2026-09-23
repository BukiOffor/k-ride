pub mod buses;
pub mod points;
pub mod routes;
pub mod bookings;

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(points::routes(state.clone()))
        .merge(buses::routes(state.clone()))
        .merge(routes::routes(state.clone()))
        .merge(bookings::routes(state))
}

pub use super::*;