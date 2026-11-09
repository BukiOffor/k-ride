//! Errors and HTTP rejection responses for admin operations.
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
#[derive(Debug)]
pub struct AdminError(pub StatusCode, pub &'static str, pub String);
impl IntoResponse for AdminError {
    /// Serialize an admin failure using the API’s code/message error envelope.
    fn into_response(self) -> Response {
        (self.0, Json(json!({"code":self.1,"message":self.2}))).into_response()
    }
}
impl From<diesel::result::Error> for AdminError {
    /// Map database failures to public errors without exposing SQL or connection details.
    fn from(e: diesel::result::Error) -> Self {
        tracing::error!(error=%e,"Admin database operation failed");
        match e {
            diesel::result::Error::NotFound => missing(),
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                _,
            ) => bad("Referenced record does not exist"),
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Database operation failed".into(),
            ),
        }
    }
}
/// Build an invalid-request error for failed input validation.
pub(crate) fn bad(s: &str) -> AdminError {
    AdminError(StatusCode::BAD_REQUEST, "invalid_request", s.into())
}
/// Build the standard error for a missing or inaccessible record.
pub(crate) fn missing() -> AdminError {
    AdminError(
        StatusCode::NOT_FOUND,
        "not_found",
        "Record not found".into(),
    )
}
/// Build a workflow-conflict error when an action cannot proceed.
pub(crate) fn conflict(s: &str) -> AdminError {
    AdminError(StatusCode::CONFLICT, "invalid_transition", s.into())
}
