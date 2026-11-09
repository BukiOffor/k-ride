//! HTTP handlers for reports.
use super::*;

/// Calculate report metrics, previous-period deltas and daily series.
#[utoipa::path(get,path="/admin/reports",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=Vec<dto::ReportSection>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub async fn reports(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::reports::reports(s, c, f).await?))
}

/// Build current operational counts, period performance metrics and the latest active SOS.
#[utoipa::path(get,path="/admin/overview",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Overview),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub async fn overview(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::reports::overview(s, c, f).await?))
}

/// Generate a CSV report and audit the export before returning its contents.
#[utoipa::path(get,path="/admin/reports/export",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="CSV download",body=String,content_type="text/csv"),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("period"=Option<String>,Query)))]
pub async fn export(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Response, AdminError> {
    let csv = services::reports::export(s, c, f).await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=paid-rides-report.csv",
            ),
        ],
        csv,
    )
        .into_response())
}
