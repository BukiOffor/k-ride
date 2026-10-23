pub mod dtos;
pub mod handlers;
pub mod models;
pub mod realtime;
pub mod services;
pub mod swagger;

pub use realtime::WsRegistry;

pub use dtos::*;
pub use models::cars::*;

// Axum re-exports for handler convenience
pub use axum::extract::*;
pub use axum::routing::{Router, delete, get, patch, post, put};

// Error type used by handlers/services
pub use crate::errors::ModuleError as ApiError;

pub use super::*;

#[cfg(test)]
mod scheduled_tests;
pub mod scheduled_worker;
