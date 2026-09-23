use super::model::ActivityType;
use crate::types::Pagination;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams, utoipa::ToSchema)]
pub struct GetActivityLogsQuery {
    pub user_id: Option<Uuid>,
    pub activity_type: Option<ActivityType>,
    pub target_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub pagination: Option<Pagination>,
}
