pub mod cars;
pub mod rides;
pub mod drivers;


pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(cars::routes(state.clone()))
        .merge(rides::routes(state.clone()))
        .merge(drivers::routes(state.clone()))
        .merge(Router::new().nest("/rides", crate::vrp::realtime::routes(state)))
}

pub use super::*;