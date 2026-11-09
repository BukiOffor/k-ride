//! HTTP adapters for network-wide scheduled bookings and manual assignment.
use super::*;
use crate::vrp::dtos::scheduled::{ScheduledRideDto, ScheduledRidePage};

/// Return scheduled bookings across the network to Admin/Root callers.
#[utoipa::path(get, path="/admin/scheduled-rides", tag="Paid Rides Admin", security(("bearerAuth"=[]),("cookieAuth"=[])), params(dto::ScheduledRideFilters), responses((status=200,description="Scheduled bookings",body=ScheduledRidePage),(status=400,description="Invalid filters"),(status=403,description="Admin or Root required")))]
pub async fn list(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<dto::ScheduledRideFilters>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::scheduled::list(s, c, f).await?))
}

/// Reserve an open scheduled booking for the selected driver without bypassing scheduling rules.
#[utoipa::path(post,path="/admin/scheduled-rides/{id}/assign",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),params(("id"=Uuid,Path)),request_body=dto::AssignScheduledDriver,responses((status=200,description="Scheduled booking reserved",body=ScheduledRideDto),(status=400,description="Invalid request"),(status=403,description="Admin or Root required, or driver ineligible"),(status=404,description="Scheduled booking not found"),(status=409,description="Stale booking, already reserved or conflicting commitment")))]
pub async fn assign(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<dto::AssignScheduledDriver>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(services::scheduled::assign(s, c, id, p).await?))
}
