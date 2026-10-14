pub mod cars;
pub mod rides;
pub mod drivers;


pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(cars::routes(state))
}

pub use super::*;