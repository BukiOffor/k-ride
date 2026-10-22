//! Request and response contracts for future rides; all money comes from a server estimate.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduleRideDto {
    pub estimate_id: Uuid,
    pub option_id: String,
    pub scheduled_at: DateTime<Utc>,
    /// IANA timezone used in confirmations, for example Africa/Lagos.
    pub timezone: String,
}
/// Version of the booking the driver reviewed, preventing acceptance of a changed pickup time.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptScheduledRideDto {
    pub version: i32,
}
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RescheduleRideDto {
    pub scheduled_at: DateTime<Utc>,
    pub timezone: String,
}
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ScheduledCancellationDto {
    pub reason: String,
}
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduledQuery {
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
    pub status: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Driver lists default to open requests; mine=true lists their reservations.
    pub mine: Option<bool>,
}
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRideDto {
    pub id: Uuid,
    pub status: String,
    pub version: i32,
    pub passenger_id: Uuid,
    pub driver_id: Option<Uuid>,
    pub ride_id: Option<Uuid>,
    pub scheduled_at: DateTime<Utc>,
    pub timezone: String,
    pub duration_seconds: i32,
    pub buffer_minutes: i32,
    pub pickup: super::rides::Place,
    pub destination: super::rides::Place,
    pub total: super::rides::Money,
    pub fare: serde_json::Value,
    pub fare_locked: bool,
    pub option: serde_json::Value,
    pub cancellation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledRidePage {
    pub items: Vec<ScheduledRideDto>,
    pub next_cursor: Option<Uuid>,
    pub total: i64,
}
