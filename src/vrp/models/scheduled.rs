//! Query result models for scheduled booking projections and delivery queues.
use diesel::{QueryableByName, sql_types::Jsonb};
#[derive(Debug, QueryableByName)]
pub(crate) struct ScheduledJsonRow {
    #[diesel(sql_type=Jsonb)]
    pub data: serde_json::Value,
}
