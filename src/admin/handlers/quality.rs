//! HTTP handlers for quality.
use super::*;

/// List open or cleared quality flags with rating aggregates.
#[utoipa::path(get,path="/admin/quality-flags",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("role"=Option<String>,Query),("status"=Option<String>,Query)))]
pub async fn quality(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::quality::quality(s, c, f).await?))
}

/// Clear a quality flag and retain its history and optional reviewer note.
#[utoipa::path(post,path="/admin/quality-flags/{id}/clear",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
pub async fn clear_flag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::quality::clear_flag(s, c, id, p).await?))
}
