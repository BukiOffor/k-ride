pub mod dtos;
pub mod handlers;
pub mod models;
pub mod services;
pub mod swagger;

pub use dtos::*;
pub use models::bookings::*;
pub use models::buses::*;
pub use models::points::*;
pub use models::ride_request::*;
pub use models::routes::*;
pub use models::trips::*;

// Axum re-exports for handler convenience
pub use axum::extract::*;
pub use axum::routing::{Router, delete, get, patch, post, put};

// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;

pub use super::*;
