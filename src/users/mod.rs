use super::*;
pub mod dto;
pub mod handlers;
pub mod model;
pub mod services;
pub mod swagger;

pub use dto::*;
pub use model::{RoleType, User};

// Axum re-exports for handler convenience
pub use axum::{extract::*, routing::*};

// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;
