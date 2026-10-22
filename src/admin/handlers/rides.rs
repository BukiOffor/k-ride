//! HTTP handlers for rides.
use super::*;

/// List network rides with server-side filtering, pagination and filter counts.
#[utoipa::path(get,path="/admin/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("state"=Option<String>,Query),("status"=Option<String>,Query),("sos"=Option<bool>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::rides::rides(s, c, f).await?))
}

/// Load one network ride, its timeline and incident-gated contact details.
#[utoipa::path(get,path="/admin/rides/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn ride(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::rides::ride(s, c, id).await?))
}

/// Manually match an unassigned ride to an eligible driver.
#[utoipa::path(post,path="/admin/rides/{id}/match",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),request_body=dto::MatchDriver,responses((status=200,description="Ride successfully matched",body=serde_json::Value),(status=400,description="Invalid request or driver ineligible"),(status=403,description="Admin or Root required"),(status=404,description="Ride not found"),(status=409,description="Ride already matched or driver busy")),params(("id"=Uuid,Path)))]
pub async fn match_driver(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<dto::MatchDriver>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::rides::match_driver(s, c, id, p.driver_id).await?))
}
