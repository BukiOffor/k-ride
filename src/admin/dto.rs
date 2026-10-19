//! Public schemas for the admin API; persisted projections use these JSON names.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplicationStatus {
    Draft,
    Submitted,
    DocumentReview,
    InspectionRequired,
    InspectionScheduled,
    FinalReview,
    Approved,
    ChangesRequired,
    Rejected,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Uuid>,
    pub total: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<Value>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PersonSummary {
    pub user_id: Uuid,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub email: String,
    pub home_area: String,
    pub track: Option<String>,
    pub verification_status: String,
    pub membership_status: String,
    pub restricted: bool,
    pub trusted_contact_count: i64,
    pub completed_rides: i64,
    pub rating: Option<f64>,
    pub vehicle: Option<Value>,
    pub availability: String,
    pub account_status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminRideSummary {
    pub id: Uuid,
    pub reference: String,
    pub status: crate::vrp::models::rides::RideStatus,
    pub ride_type: String,
    pub track: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub eta_seconds: Option<f64>,
    pub pickup: Value,
    pub destination: Value,
    pub driver: Option<Value>,
    pub passengers: Vec<Value>,
    pub sos: Value,
    pub driver_location: Option<Value>,
    pub total: Value,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationSummary {
    pub user_id: Uuid,
    pub applicant_name: String,
    pub avatar_url: Option<String>,
    pub track: String,
    pub status: ApplicationStatus,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub documents_ready: i64,
    pub documents_total: i64,
    pub vehicle: Value,
    pub inspection: Option<Value>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDocument {
    pub r#type: String,
    pub label: String,
    pub status: String,
    pub base64: Option<String>,
    pub note: Option<String>,
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::NaiveDate>,
    pub mime_type: String,
    pub size_bytes: i64,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    #[serde(flatten)]
    pub summary: ApplicationSummary,
    pub documents: Vec<ApplicationDocument>,
    pub licence: Value,
    pub affiliation: Value,
    pub nin_verified: bool,
    pub comments: Vec<Value>,
    pub changes_requested: Vec<String>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SosAlert {
    pub id: Uuid,
    pub ride_id: Uuid,
    pub ride_reference: String,
    pub status: String,
    pub r#type: String,
    pub raised_by: Value,
    pub raised_at: chrono::DateTime<chrono::Utc>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub location: Option<Value>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    pub id: Uuid,
    pub reference: String,
    pub ride_id: Option<Uuid>,
    pub ride_reference: Option<String>,
    pub category: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub description: String,
    pub assigned_to: Option<Value>,
    pub opened_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub participants: Vec<Value>,
    #[serde(default)]
    pub timeline: Vec<Value>,
    #[serde(default)]
    pub admin_notes: Vec<Value>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub id: String,
    pub value: Option<f64>,
    pub label: Option<String>,
    pub delta_percent: Option<f64>,
    pub series: Option<Vec<f64>>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub primary: Vec<Metric>,
    pub secondary: Vec<Metric>,
    pub active_sos: Option<SosAlert>,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportSection {
    pub id: String,
    pub metrics: Vec<Metric>,
    pub series: Option<Vec<Value>>,
}

use super::errors::{AdminError, bad};
use super::services::member;

#[derive(Default, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
    pub q: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub category: Option<String>,
    pub track: Option<String>,
    pub availability: Option<String>,
    pub verification: Option<String>,
    pub flagged: Option<bool>,
    pub active: Option<bool>,
    pub membership: Option<String>,
    pub has_trusted_contact: Option<bool>,
    pub sos: Option<bool>,
    pub role: Option<String>,
    pub period: Option<String>,
}

#[derive(Default, Deserialize, utoipa::ToSchema)]
pub struct Note {
    pub note: Option<String>,
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewIncident {
    pub ride_id: Option<Uuid>,
    pub category: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub participant_ids: Vec<Uuid>,
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncidentPatch {
    pub status: Option<String>,
    pub severity: Option<String>,
    pub assignee_id: Option<Uuid>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    pub participant_id: Uuid,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Reason {
    pub reason: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct Contact {
    pub name: String,
    pub relationship: String,
    pub phone: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DocumentReview {
    pub status: String,
    pub note: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub body: String,
    pub visibility: String,
    pub document_type: Option<String>,
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    pub location: String,
    pub note: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub decision: String,
    pub reason: Option<String>,
    #[serde(default)]
    pub changes_requested: Vec<String>,
}

impl Filters {
    /// Validate paging/filter inputs and encode the parameters used by SQL projections.
    pub(crate) fn params(&self) -> Result<Value, AdminError> {
        if self.limit.is_some_and(|n| !(1..=100).contains(&n)) {
            return Err(bad("limit must be between 1 and 100"));
        }
        if let Some(ref state) = self.state {
            member(state, &["active", "all"])?;
        }
        let mut v = serde_json::to_value(self).unwrap();
        v["limit"] = json!(self.limit.unwrap_or(25));
        Ok(v)
    }
}
