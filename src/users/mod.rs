use super::*;
pub mod dto;
pub mod model;
pub mod services;
pub mod handlers;
pub mod swagger;

pub use model::{User, RoleType};
pub use dto::*;

// Axum re-exports for handler convenience
pub use axum::{extract::*, routing::*};


// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;