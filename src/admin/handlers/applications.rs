//! HTTP handlers for applications.
use super::*;

/// List driver applications without loading document bodies into the queue.
#[utoipa::path(get,path="/admin/driver-applications",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::ApplicationSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query),("track"=Option<String>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::list(s, c, f).await?))
}

/// Load the complete driver application for an authorized admin reviewer.
#[utoipa::path(get,path="/admin/driver-applications/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Application),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn detail(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::detail(s, c, id).await?))
}

/// Return the driver’s own application feedback without internal reviewer comments.
#[utoipa::path(get,path="/rides/me/driver-review",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")))]
pub async fn own_review(
    State(s): State<Arc<AppState>>,
    c: Claims,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::own_review(s, c).await?))
}

/// Review a document, validating its presence, expiry and any required reason.
#[utoipa::path(patch,path="/admin/driver-applications/{id}/documents/{doc}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::ApplicationDocument),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=DocumentReview,params(("id"=Uuid,Path),("doc"=String,Path)))]
pub async fn document(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path((id, doc)): Path<(Uuid, String)>,
    Json(p): Json<DocumentReview>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(
        services::applications::document(s, c, id, doc, p).await?,
    ))
}

/// Add an attributed application comment with explicit internal or applicant visibility.
#[utoipa::path(post,path="/admin/driver-applications/{id}/comments",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Comment,params(("id"=Uuid,Path)))]
pub async fn comment(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Comment>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::comment(s, c, id, p).await?))
}

/// Schedule an application inspection and advance its review state.
#[utoipa::path(post,path="/admin/driver-applications/{id}/inspection",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Inspection,params(("id"=Uuid,Path)))]
pub async fn inspection(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Inspection>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::inspection(s, c, id, p).await?))
}

/// Schedule a periodic driver inspection without changing application approval.
#[utoipa::path(post,path="/admin/drivers/{id}/inspection",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Inspection,params(("id"=Uuid,Path)))]
pub async fn driver_inspection(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Inspection>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(
        services::applications::driver_inspection(s, c, id, p).await?,
    ))
}

/// Validate an application decision and commit it with its audit trail.
#[utoipa::path(post,path="/admin/driver-applications/{id}/decision",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Decision,params(("id"=Uuid,Path)))]
pub async fn decision(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Decision>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::applications::decision(s, c, id, p).await?))
}
