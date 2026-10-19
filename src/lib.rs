pub mod admin;
pub mod auth;
pub mod bus;
pub mod config;
pub mod errors;
pub mod helpers;
pub mod logs;
pub mod macros;
pub mod mailer;
pub mod schema;
pub mod traits;
pub mod types;
pub mod users;
pub mod vrp;

// ==================================================================================================================================================================
// ==================================================================================================================================================================
// =============================================================== Request & Std Libraries ==========================================================================
// ==================================================================================================================================================================
// ==================================================================================================================================================================
pub use self::errors::ModuleError;
pub use axum::Router;
use axum::middleware;
pub use axum::{
    extract::*,
    routing::{delete, get, patch, post, put},
};
use dispatcher::RideDispatcher;
pub use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;
pub use uuid::Uuid;
// ==================================================================================================================================================================
// ==================================================================================================================================================================
// =============================================================== Database and App State ===========================================================================
// ==================================================================================================================================================================
// ==================================================================================================================================================================
pub use chrono::NaiveDateTime;
pub use diesel::backend::Backend;
pub use diesel::deserialize::{self, FromSql, FromSqlRow};
pub use diesel::expression::AsExpression;
pub use diesel::pg::Pg;
pub use diesel::prelude::*;
pub use diesel::serialize::{self, Output, ToSql};
pub use diesel::sql_types::Text;
pub use diesel_async::AsyncPgConnection;
pub use diesel_async::pooled_connection::AsyncDieselConnectionManager;
pub use tracing::info;
pub type Pool = bb8::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>;
pub type Connection<'a> =
    bb8::PooledConnection<'a, AsyncDieselConnectionManager<AsyncPgConnection>>;
pub const POOL_ERROR_MSG: &str = "Could not get connection from the database pool";
pub type Logger = async_channel::Sender<logs::ActivityLog>;

#[derive(Clone)]
pub struct AppState {
    pub pool: Arc<Pool>,
    pub logger: Logger,
    pub dispatcher: Arc<RideDispatcher>,
    pub ws_registry: Arc<vrp::realtime::WsRegistry>,
}
// ==================================================================================================================================================================
// ==================================================================================================================================================================
// ================================================================== Other Libraries ===============================================================================
// ==================================================================================================================================================================
// ==================================================================================================================================================================
pub use serde_json;
pub use std::error::Error;

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Message<T> {
    pub message: String,
    pub data: T,
}

impl<T> Message<T> {
    pub fn context(message: &str, data: T) -> Self {
        Self {
            message: message.to_string(),
            data,
        }
    }
}
