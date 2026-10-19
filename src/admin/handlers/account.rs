//! HTTP handlers for account.
use super::*;

/// Page through durable notifications belonging to the authenticated user.
#[utoipa::path(get,path="/rides/me/notifications",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn notifications(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::account::notifications(s, c, f).await?))
}

/// Read the authenticated user’s trusted contacts, including their phone numbers.
#[utoipa::path(get,path="/rides/me/trusted-contacts",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")))]
pub async fn contacts(
    State(s): State<Arc<AppState>>,
    c: Claims,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::account::contacts(s, c).await?))
}

/// Validate and save a trusted contact for the authenticated user.
#[utoipa::path(post,path="/rides/me/trusted-contacts",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=201,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Contact)]
pub async fn add_contact(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<Contact>,
) -> Result<(StatusCode, Json<Value>), AdminError> {
    Ok((
        StatusCode::CREATED,
        Json(services::account::add_contact(s, c, p).await?),
    ))
}

/// Delete a trusted contact only when it belongs to the authenticated user.
#[utoipa::path(delete,path="/rides/me/trusted-contacts/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn remove_contact(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::account::remove_contact(s, c, id).await?))
}
