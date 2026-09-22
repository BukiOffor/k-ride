use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use thiserror::Error;

use crate::mailer::types::MailerEvent;

#[derive(Error, Debug)]
pub enum ModuleError {
    #[error("{0}")]
    DieselError(#[from] diesel::result::Error),

    #[error("{0}")]
    PoolError(#[from] diesel_async::pooled_connection::deadpool::PoolError),

    #[error("{0}")]
    AxumError(#[from] axum::Error),

    #[error("{0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("{0}")]
    IOError(#[from] std::io::Error),

    #[error("{0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("{0}")]
    JsonWebTokenError(#[from] jsonwebtoken::errors::Error),

    #[error("{0}")]
    VarError(#[from] std::env::VarError),

    #[error("{0}")]
    SendMailError(#[from] async_channel::SendError<MailerEvent>),

    #[error("{0}")]
    PoolRunError(#[from] diesel_async::pooled_connection::bb8::RunError),

    #[error("Internal error: {0}")]
    InternalError(Cow<'static, str>),

    #[error("User does not have access to this service")]
    PermissionDenied,

    #[error("Provided Token is Invalid")]
    InvalidToken,

    #[error("Missing Credentials")]
    MissingCredentials,

    #[error("{0}")]
    CouldNotExtractToken(&'static str),

    #[error("Invalid credentials")]
    AuthError,

    #[error("Invalid credentials")]
    WrongCredentials,

    #[error("data not available in database")]
    ItemNotFound,

    #[error("{0}")]
    Error(Cow<'static, str>),

    #[error("Not allowed: {0}")]
    NotAllowed(&'static str),

    #[error("Bad request: {0}")]
    BadRequest(&'static str),

    #[error("Not found: {0}")]
    NotFoundError(std::borrow::Cow<'static, str>),
}

#[derive(Debug, Default, Error, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ErrorMessage {
    pub message: String,
    pub status_code: u32,
}

impl std::fmt::Display for ErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl ErrorMessage {
    pub fn build(&mut self, message: String, status_code: u32) -> Self {
        Self {
            message,
            status_code,
        }
    }
}

impl IntoResponse for ModuleError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::WrongCredentials | Self::InvalidToken | Self::AuthError => {
                let message = ErrorMessage::default().build(self.to_string(), 401);
                (axum::http::StatusCode::UNAUTHORIZED, axum::Json(message)).into_response()
            }
            Self::PermissionDenied | Self::NotAllowed(_) => {
                let message = ErrorMessage::default().build(self.to_string(), 403);
                (axum::http::StatusCode::FORBIDDEN, axum::Json(message)).into_response()
            }
            Self::DieselError(_) | Self::PoolError(_) | Self::InternalError(_) => {
                let message = ErrorMessage::default().build(self.to_string(), 500);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(message),
                )
                    .into_response()
            }
            Self::CouldNotExtractToken(_)
            | Self::ItemNotFound
            | Self::Error(_)
            | Self::BadRequest(_) => {
                let message = ErrorMessage::default().build(self.to_string(), 400);
                (axum::http::StatusCode::BAD_REQUEST, axum::Json(message)).into_response()
            }
            Self::NotFoundError(_) => {
                let message = ErrorMessage::default().build(self.to_string(), 404);
                (axum::http::StatusCode::NOT_FOUND, axum::Json(message)).into_response()
            }
            _ => {
                let message = ErrorMessage::default().build(self.to_string(), 500);
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(message),
                )
                    .into_response()
            }
        }
    }
}
