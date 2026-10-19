//! HTTP handlers for people.
use super::*;

/// List drivers with vehicle, verification and account oversight summaries.
#[utoipa::path(get,path="/admin/drivers",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::PersonSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("q"=Option<String>,Query),("track"=Option<String>,Query),("availability"=Option<String>,Query),("verification"=Option<String>,Query),("flagged"=Option<bool>,Query),("active"=Option<bool>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn drivers(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::drivers(s, c, f).await?))
}

/// List all drivers currently live in the dispatch index with their latest coordinates.
#[utoipa::path(get,path="/admin/drivers/live",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Live drivers with latest position",body=serde_json::Value),(status=403,description="Admin or Root required"),(status=503,description="Live driver index unavailable")))]
pub async fn live_drivers(
    State(s): State<Arc<AppState>>,
    c: Claims,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::live_drivers(s, c).await?))
}

/// List passengers with membership, ride and trusted-contact summaries.
#[utoipa::path(get,path="/admin/passengers",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::PersonSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("q"=Option<String>,Query),("membership"=Option<String>,Query),("hasTrustedContact"=Option<bool>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn passengers(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::passengers(s, c, f).await?))
}

/// Load a driver record, activity statistics, affiliation and vehicle information.
#[utoipa::path(get,path="/admin/drivers/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn driver(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::driver(s, c, id).await?))
}

/// Load a passenger record, ride statistics and trusted-contact summaries.
#[utoipa::path(get,path="/admin/passengers/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn passenger(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::passenger(s, c, id).await?))
}

/// Return only the selected driver’s ride history.
#[utoipa::path(get,path="/admin/drivers/{id}/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn driver_rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::driver_rides(s, c, id, f).await?))
}

/// Return only the selected passenger’s ride history.
#[utoipa::path(get,path="/admin/passengers/{id}/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn passenger_rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::passenger_rides(s, c, id, f).await?))
}

/// Create or update a manual driver quality flag with a required reason.
#[utoipa::path(post,path="/admin/drivers/{id}/flag",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
pub async fn flag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::flag(s, c, id, p).await?))
}

/// Clear the driver’s open quality flag without deleting ratings.
#[utoipa::path(delete,path="/admin/drivers/{id}/flag",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn unflag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::unflag(s, c, id).await?))
}

/// Deactivate a driver, withdraw offers and remove them from live dispatch.
#[utoipa::path(post,path="/admin/drivers/{id}/deactivate",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
pub async fn deactivate(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::deactivate(s, c, id, p).await?))
}

/// Block new ride requests for a passenger while preserving ongoing trips.
#[utoipa::path(post,path="/admin/passengers/{id}/restriction",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
pub async fn restrict(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::restrict(s, c, id, p).await?))
}

/// Lift the passenger’s ride-request restriction.
#[utoipa::path(delete,path="/admin/passengers/{id}/restriction",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn unrestrict(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::unrestrict(s, c, id).await?))
}

/// Mark a passenger membership check as pending provider verification.
#[utoipa::path(post,path="/admin/passengers/{id}/membership-check",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn membership_check(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::people::membership_check(s, c, id).await?))
}
