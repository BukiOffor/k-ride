//! Paid rides administration: HTTP adapters, business services and public contracts.
pub mod dto;
pub mod errors;
pub mod handlers;
pub mod model;
pub mod services;
pub mod swagger;

#[cfg(test)]
mod tests;
