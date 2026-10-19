//! Database result models used by the admin SQL projections.
use diesel::{QueryableByName, sql_types::Jsonb};
use serde_json::Value;
#[derive(QueryableByName)]
pub(crate) struct Row {
    #[diesel(sql_type=Jsonb)]
    pub(crate) data: Value,
}
