pub mod dtos;
pub mod handlers;
pub mod models;
pub mod services;
pub mod swagger;

pub use dtos::*;
pub use models::{buses::*, points::*};

// Axum re-exports for handler convenience
pub use axum::{extract::*, routing::*};

// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;

pub use super::*;