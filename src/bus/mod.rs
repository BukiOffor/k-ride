pub mod dtos;
pub mod handlers;
pub mod models;
pub mod services;
pub mod swagger;

pub use dtos::*;
pub use models::{bookings::*, buses::*, points::*, routes::*};

// Axum re-exports for handler convenience
pub use axum::{
    extract::*,
    routing::{Router, delete, get, patch, post, put},
};

// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;

pub use super::*;
