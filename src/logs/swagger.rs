use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(super::handlers::get_logs, super::handlers::get_log_by_id,),
    components(schemas(
        super::model::ActivityLog,
        super::model::ActivityType,
        super::dto::GetActivityLogsQuery,
    ))
)]
pub struct LogsApiDoc;
