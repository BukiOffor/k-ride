//! HTTP handlers for safety.
use super::*;

/// List SOS alerts, optionally restricted to active alerts.
#[utoipa::path(get,path="/admin/sos",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=Vec<dto::SosAlert>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query)))]
pub async fn sos(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::sos(s, c, f).await?))
}

/// Acknowledge an SOS and synchronize the ride safety state.
#[utoipa::path(post,path="/admin/sos/{id}/acknowledge",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::SosAlert),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn acknowledge(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::acknowledge(s, c, id).await?))
}

/// Resolve an SOS while keeping its incident available for further review.
#[utoipa::path(post,path="/admin/sos/{id}/resolve",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::SosAlert),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
pub async fn resolve(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(n): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::resolve(s, c, id, n).await?))
}

/// List incidents with filters, pagination and status counts.
#[utoipa::path(get,path="/admin/incidents",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::Incident>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query),("severity"=Option<String>,Query),("category"=Option<String>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn incidents(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::incidents(s, c, f).await?))
}

/// Load an incident, participants, timeline and reviewer notes.
#[utoipa::path(get,path="/admin/incidents/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::incident(s, c, id).await?))
}

/// Validate and open a manual incident, recording an audit event.
#[utoipa::path(post,path="/admin/incidents",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=201,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=NewIncident)]
pub async fn create_incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<NewIncident>,
) -> Result<(StatusCode, Json<Value>), AdminError> {
    Ok((
        StatusCode::CREATED,
        Json(services::safety::create_incident(s, c, p).await?),
    ))
}

/// Update incident status, severity or assignment and return refreshed detail.
#[utoipa::path(patch,path="/admin/incidents/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=IncidentPatch,params(("id"=Uuid,Path)))]
pub async fn update_incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<IncidentPatch>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::update_incident(s, c, id, p).await?))
}

/// Append an attributed admin note to an incident and record the action.
#[utoipa::path(post,path="/admin/incidents/{id}/notes",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
pub async fn add_note(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::add_note(s, c, id, p).await?))
}

/// Log a call initiation and release the selected open-incident participant’s tel URL.
#[utoipa::path(post,path="/admin/incidents/{id}/calls",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),request_body=Call,params(("id"=Uuid,Path)),responses((status=200,description="Logged call initiation, with phone and tel URL",body=serde_json::Value),(status=404,description="Open incident or participant not found")))]
pub async fn log_call(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Call>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::safety::log_call(s, c, id, p).await?))
}
