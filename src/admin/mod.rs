//! Paid rides administration. SQL projections keep document bodies out of list responses.
use crate::{AppState, Connection, ModuleError, auth::jwt::Claims};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use diesel::{
    QueryableByName,
    sql_types::{Jsonb, Uuid as SqlUuid},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

mod applications;
pub mod dto;
pub use applications::own_review;
mod reports;

#[derive(Debug)]
pub struct AdminError(pub StatusCode, pub &'static str, pub String);
impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"code":self.1,"message":self.2}))).into_response()
    }
}
impl From<diesel::result::Error> for AdminError {
    fn from(e: diesel::result::Error) -> Self {
        tracing::error!(error=%e,"Admin database operation failed");
        match e {
            diesel::result::Error::NotFound => missing(),
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                _,
            ) => bad("Referenced record does not exist"),
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Database operation failed".into(),
            ),
        }
    }
}
fn bad(s: &str) -> AdminError {
    AdminError(StatusCode::BAD_REQUEST, "invalid_request", s.into())
}
fn missing() -> AdminError {
    AdminError(
        StatusCode::NOT_FOUND,
        "not_found",
        "Record not found".into(),
    )
}
fn conflict(s: &str) -> AdminError {
    AdminError(StatusCode::CONFLICT, "invalid_transition", s.into())
}
fn require_admin(c: &Claims) -> Result<(), AdminError> {
    if matches!(
        c.role,
        crate::users::RoleType::Admin | crate::users::RoleType::Root
    ) {
        Ok(())
    } else {
        Err(AdminError(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Admin or Root role required".into(),
        ))
    }
}
#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
async fn query(
    conn: &mut Connection<'_>,
    sql: &str,
    p: Value,
    id: Uuid,
) -> Result<Value, AdminError> {
    Ok(diesel::sql_query(sql)
        .bind::<Jsonb, _>(p)
        .bind::<SqlUuid, _>(id)
        .get_result::<Row>(conn)
        .await?
        .data)
}
async fn connection(state: &AppState) -> Result<Connection<'_>, AdminError> {
    state.pool.get().await.map_err(|e| {
        tracing::error!(error=%e,"Admin pool unavailable");
        AdminError(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "Database unavailable".into(),
        )
    })
}

#[derive(Default, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    cursor: Option<Uuid>,
    limit: Option<i64>,
    q: Option<String>,
    state: Option<String>,
    status: Option<String>,
    severity: Option<String>,
    category: Option<String>,
    track: Option<String>,
    availability: Option<String>,
    verification: Option<String>,
    flagged: Option<bool>,
    active: Option<bool>,
    membership: Option<String>,
    has_trusted_contact: Option<bool>,
    sos: Option<bool>,
    role: Option<String>,
    period: Option<String>,
}
impl Filters {
    fn params(&self) -> Result<Value, AdminError> {
        if self.limit.is_some_and(|n| !(1..=100).contains(&n)) {
            return Err(bad("limit must be between 1 and 100"));
        }
        if let Some(ref state) = self.state {
            member(state, &["active", "all"])?;
        }
        let mut v = serde_json::to_value(self).unwrap();
        v["limit"] = json!(self.limit.unwrap_or(25));
        Ok(v)
    }
}
fn page_sql(source: &str, predicate: &str) -> String {
    format!(
        "WITH filtered AS (SELECT id,data FROM {source} WHERE ({predicate}) AND $2 IS NOT NULL), page AS (SELECT id,data FROM filtered WHERE ($1->>'cursor' IS NULL OR id < ($1->>'cursor')::uuid) ORDER BY id DESC LIMIT (($1->>'limit')::int+1)), visible AS (SELECT * FROM page ORDER BY id DESC LIMIT ($1->>'limit')::int) SELECT jsonb_build_object('items',COALESCE((SELECT jsonb_agg(data ORDER BY id DESC) FROM visible),'[]'::jsonb),'total',(SELECT count(*) FROM filtered),'nextCursor',CASE WHEN (SELECT count(*) FROM page)>($1->>'limit')::int THEN (SELECT id FROM visible ORDER BY id LIMIT 1) END) AS data"
    )
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/overview", get(reports::overview))
        .route("/reports", get(reports::reports))
        .route("/reports/export", get(reports::export))
        .route("/rides", get(rides))
        .route("/rides/{id}", get(ride))
        .route("/sos", get(sos))
        .route("/sos/{id}/acknowledge", post(acknowledge))
        .route("/sos/{id}/resolve", post(resolve))
        .route("/incidents", get(incidents).post(create_incident))
        .route("/incidents/{id}", get(incident).patch(update_incident))
        .route("/incidents/{id}/notes", post(add_note))
        .route("/incidents/{id}/calls", post(log_call))
        .route("/drivers", get(drivers))
        .route("/drivers/{id}", get(driver))
        .route("/drivers/{id}/rides", get(driver_rides))
        .route("/drivers/{id}/flag", post(flag).delete(unflag))
        .route("/drivers/{id}/deactivate", post(deactivate))
        .route(
            "/drivers/{id}/inspection",
            post(applications::driver_inspection),
        )
        .route("/passengers", get(passengers))
        .route("/passengers/{id}", get(passenger))
        .route("/passengers/{id}/rides", get(passenger_rides))
        .route(
            "/passengers/{id}/restriction",
            post(restrict).delete(unrestrict),
        )
        .route("/passengers/{id}/membership-check", post(membership_check))
        .route("/quality-flags", get(quality))
        .route("/quality-flags/{id}/clear", post(clear_flag))
        .route("/driver-applications", get(applications::list))
        .route("/driver-applications/{id}", get(applications::detail))
        .route(
            "/driver-applications/{id}/documents/{doc}",
            patch(applications::document),
        )
        .route(
            "/driver-applications/{id}/comments",
            post(applications::comment),
        )
        .route(
            "/driver-applications/{id}/inspection",
            post(applications::inspection),
        )
        .route(
            "/driver-applications/{id}/decision",
            post(applications::decision),
        )
        .route_layer(axum::middleware::from_fn(admin_gate))
        .with_state(state)
}
async fn admin_gate(
    c: Claims,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, AdminError> {
    require_admin(&c)?;
    Ok(next.run(req).await)
}

const RIDE_FILTER: &str = "($1->>'state' IS DISTINCT FROM 'active' OR status IN ('SEARCHING','DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED')) AND ($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'sos' IS NULL OR ((data->'sos'->>'status') IN ('sent','acknowledged','responding'))=($1->>'sos')::boolean) AND ($1->>'q' IS NULL OR concat_ws(' ',data->>'reference',data->'driver'->>'displayName',data->'passengers',data->'destination'->>'label',data->'destination'->>'name') ILIKE '%' || ($1->>'q') || '%')";
#[utoipa::path(get,path="/admin/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("state"=Option<String>,Query),("status"=Option<String>,Query),("sos"=Option<bool>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let mut result=query(&mut db,&page_sql("(SELECT id,created_at,status,driver_id,passenger_id,data-'route'-'fare'-'payment'-'option'-'rating'-'cancellation'-'progress'-'approach'-'timestamps' AS data FROM admin_rides_view) summaries",RIDE_FILTER),f.params()?,c.user_id).await?;
    result["counts"]=query(&mut db,"SELECT jsonb_build_object('all',count(*),'sos',count(*) FILTER(WHERE data->'sos'->>'status' IN ('sent','acknowledged','responding')),'inProgress',count(*) FILTER(WHERE status IN ('IN_PROGRESS','PAUSED')),'searching',count(*) FILTER(WHERE status IN ('REQUESTED','SEARCHING'))) AS data FROM admin_rides_view WHERE ($1->>'state' IS DISTINCT FROM 'active' OR status IN ('SEARCHING','DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED')) AND $2 IS NOT NULL",f.params()?,c.user_id).await?;
    Ok(Json(result))
}
#[utoipa::path(get,path="/admin/rides/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn ride(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let mut result = query(
        &mut db,
        "SELECT data FROM admin_rides_view WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        id,
    )
    .await?;
    result["events"]=query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'type',event_type,'label',event_type,'detail',detail,'at',created_at AT TIME ZONE 'UTC','kind',CASE WHEN event_type LIKE 'sos.%' OR event_type LIKE 'incident.%' THEN 'safety' ELSE 'status' END) ORDER BY created_at,id),'[]'::jsonb) AS data FROM admin_events WHERE (target_id=$2 OR target_id IN (SELECT id FROM safety_events WHERE ride_id=$2)) AND $1 IS NOT NULL",json!({}),id).await?;
    let phones=query(&mut db,"SELECT CASE WHEN EXISTS(SELECT 1 FROM admin_incidents WHERE ride_id=$2 AND status<>'RESOLVED') THEN jsonb_build_object('driver',d.phone,'passenger',p.phone) ELSE '{}'::jsonb END AS data FROM rides r JOIN users p ON p.id=r.passenger_id LEFT JOIN users d ON d.id=r.driver_id WHERE r.id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    if let Some(p) = phones.get("driver") {
        if result["driver"].is_object() {
            result["driver"]["phone"] = p.clone();
        }
    }
    if let Some(p) = phones.get("passenger") {
        result["passengers"][0]["phone"] = p.clone();
    }
    let alert=query(&mut db,"SELECT COALESCE((SELECT data FROM admin_sos_view WHERE ride_id=$2 ORDER BY created_at DESC,id DESC LIMIT 1),'null'::jsonb) AS data WHERE $1 IS NOT NULL",json!({}),id).await?;
    if !alert.is_null() {
        result["sos"] = alert;
    }
    Ok(Json(result))
}
#[utoipa::path(get,path="/admin/sos",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=Vec<dto::SosAlert>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query)))]
async fn sos(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(Json(query(&mut db,"SELECT COALESCE(jsonb_agg(data ORDER BY created_at DESC,id DESC),'[]'::jsonb) AS data FROM admin_sos_view WHERE ($1->>'status' IS NULL OR ($1->>'status'='active' AND status IN ('sent','acknowledged','responding')) OR status=$1->>'status') AND $2 IS NOT NULL",f.params()?,c.user_id).await?))
}

// Each mutation and its durable audit record commit together.
async fn audit(
    db: &mut Connection<'_>,
    actor: Uuid,
    target: Uuid,
    event: &str,
    detail: Value,
) -> Result<(), AdminError> {
    let activity = match event.split('.').next().unwrap_or("") {
        "sos" => "RideSafetyAction",
        "incident" => "IncidentAction",
        "verification" => "DriverVerificationAction",
        "inspection" => "DriverInspectionAction",
        "driver" => "DriverOversightAction",
        "passenger" => "PassengerRestrictionAction",
        "membership" => "MembershipCheckAction",
        "quality" => "QualityFlagAction",
        "reports" => "ReportExportAction",
        _ => "AdminAction",
    };
    let p = json!({"actor":actor,"event":event,"detail":detail,"id":Uuid::now_v7(),"activity":activity});
    query(db,"WITH ev AS (INSERT INTO admin_events(id,actor_id,target_id,event_type,detail) VALUES (($1->>'id')::uuid,($1->>'actor')::uuid,$2,$1->>'event',$1->'detail') RETURNING *) INSERT INTO activity_logs(id,user_id,activity_type,target_id,target_type,details,created_at) SELECT id,actor_id,to_jsonb($1->>'activity')::text,target_id,'paid_rides',jsonb_build_object('event',event_type,'detail',detail),created_at FROM ev RETURNING details AS data",p,target).await?;
    Ok(())
}
async fn mutate(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    event: &str,
    sql: &str,
    p: Value,
) -> Result<Value, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    let result = db
        .transaction::<Value, AdminError, _>(|db| {
            Box::pin(async move {
                let data = query(db, sql, p.clone(), id).await?;
                audit(db, c.user_id, id, event, p).await?;
                Ok(data)
            })
        })
        .await?;
    if event.starts_with("sos.") || event.starts_with("incident.") {
        let published = if event.starts_with("sos.") {
            "sos.updated"
        } else if event == "incident.opened" {
            event
        } else {
            "incident.updated"
        };
        s.ws_registry.broadcast_admin_event(
            "admin:safety",
            published,
            json!({"id":id,"action":event,"data":result}),
        );
    }
    Ok(result)
}
#[utoipa::path(post,path="/admin/sos/{id}/acknowledge",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::SosAlert),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn acknowledge(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    mutate(&s,&c,id,"sos.acknowledged","WITH e AS (UPDATE safety_events SET status=CASE WHEN status='sent' THEN 'acknowledged' ELSE status END WHERE id=$2 AND kind='sos' AND $1 IS NOT NULL RETURNING *), r AS (UPDATE rides SET sos_status=CASE WHEN rides.sos_status='sent' THEN 'acknowledged' ELSE rides.sos_status END WHERE id IN(SELECT ride_id FROM e)) SELECT jsonb_build_object('id',id) AS data FROM e",json!({})).await?;
    let mut db = connection(&s).await?;
    Ok(Json(
        query(
            &mut db,
            "SELECT data FROM admin_sos_view WHERE id=$2 AND $1 IS NOT NULL",
            json!({}),
            id,
        )
        .await?,
    ))
}
#[derive(Default, Deserialize, utoipa::ToSchema)]
struct Note {
    note: Option<String>,
}
#[utoipa::path(post,path="/admin/sos/{id}/resolve",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::SosAlert),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
async fn resolve(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(n): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    mutate(&s,&c,id,"sos.resolved","WITH e AS (UPDATE safety_events SET status='resolved' WHERE id=$2 AND kind='sos' RETURNING *), r AS (UPDATE rides SET sos_status=CASE WHEN EXISTS(SELECT 1 FROM safety_events other WHERE other.ride_id=rides.id AND other.kind='sos' AND other.id<>$2 AND other.status<>'resolved') THEN 'sent' ELSE 'resolved' END WHERE id IN (SELECT ride_id FROM e)) SELECT jsonb_build_object('id',id,'status',status) AS data FROM e WHERE $1 IS NOT NULL",json!({"note":n.note})).await?;
    let mut db = connection(&s).await?;
    Ok(Json(
        query(
            &mut db,
            "SELECT data FROM admin_sos_view WHERE id=$2 AND $1 IS NOT NULL",
            json!({}),
            id,
        )
        .await?,
    ))
}
#[utoipa::path(get,path="/admin/incidents",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::Incident>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("status"=Option<String>,Query),("severity"=Option<String>,Query),("category"=Option<String>,Query),("q"=Option<String>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn incidents(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let sql = page_sql(
        "admin_incidents_view",
        "($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'severity' IS NULL OR severity=$1->>'severity') AND ($1->>'category' IS NULL OR category=$1->>'category') AND ($1->>'q' IS NULL OR concat_ws(' ',data->>'reference',data->>'title') ILIKE '%' || ($1->>'q') || '%')",
    );
    let mut v = query(&mut db, &sql, f.params()?, c.user_id).await?;
    v["counts"]=query(&mut db,"SELECT jsonb_build_object('all',count(*),'open',count(*) FILTER(WHERE status='OPEN'),'underReview',count(*) FILTER(WHERE status='UNDER_REVIEW'),'resolved',count(*) FILTER(WHERE status='RESOLVED'),'high',count(*) FILTER(WHERE severity IN ('HIGH','CRITICAL'))) AS data FROM admin_incidents WHERE $1 IS NOT NULL AND $2 IS NOT NULL",json!({}),c.user_id).await?;
    Ok(Json(v))
}
async fn incident_data(db: &mut Connection<'_>, id: Uuid) -> Result<Value, AdminError> {
    let mut v = query(
        db,
        "SELECT data FROM admin_incidents_view WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        id,
    )
    .await?;
    v["participants"]=query(db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('userId',u.id,'displayName',u.first_name || ' ' || u.last_name,'role',u.role) || CASE WHEN i.status<>'RESOLVED' THEN jsonb_build_object('phone',u.phone) ELSE '{}'::jsonb END),'[]'::jsonb) AS data FROM admin_incidents i LEFT JOIN rides r ON r.id=i.ride_id JOIN users u ON u.id=ANY(i.participant_ids) OR u.id=r.passenger_id OR u.id=r.driver_id WHERE i.id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    v["adminNotes"]=query(db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',n.id,'author',jsonb_build_object('userId',u.id,'displayName',u.first_name || ' ' || u.last_name),'note',n.note,'at',n.created_at AT TIME ZONE 'UTC') ORDER BY n.created_at,n.id),'[]'::jsonb) AS data FROM admin_incident_notes n JOIN users u ON u.id=n.author_id WHERE n.incident_id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    v["timeline"]=query(db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'label',event_type,'detail',detail,'at',created_at AT TIME ZONE 'UTC','kind','safety') ORDER BY created_at,id),'[]'::jsonb) AS data FROM admin_events WHERE target_id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    Ok(v)
}
#[utoipa::path(get,path="/admin/incidents/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(Json(incident_data(&mut db, id).await?))
}
const CATEGORIES: &[&str] = &[
    "SAFETY_CONCERN",
    "SOS_ACTIVATION",
    "DRIVER_CONDUCT",
    "PASSENGER_CONDUCT",
    "VEHICLE_ISSUE",
    "PAYMENT_DISPUTE",
    "OTHER",
];
fn member(v: &str, set: &[&str]) -> Result<(), AdminError> {
    if set.contains(&v) {
        Ok(())
    } else {
        Err(bad("Invalid enum value"))
    }
}
fn text(v: &str) -> Result<(), AdminError> {
    if v.trim().is_empty() || v.len() > 10000 {
        Err(bad("Text must be nonempty and at most 10000 bytes"))
    } else {
        Ok(())
    }
}
#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct NewIncident {
    ride_id: Option<Uuid>,
    category: String,
    severity: String,
    title: String,
    description: String,
    #[serde(default)]
    participant_ids: Vec<Uuid>,
}
#[utoipa::path(post,path="/admin/incidents",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=201,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=NewIncident)]
async fn create_incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<NewIncident>,
) -> Result<(StatusCode, Json<Value>), AdminError> {
    member(&p.category, CATEGORIES)?;
    member(&p.severity, &["LOW", "MEDIUM", "HIGH", "CRITICAL"])?;
    text(&p.title)?;
    text(&p.description)?;
    let id = Uuid::now_v7();
    mutate(&s,&c,id,"incident.opened","INSERT INTO admin_incidents(id,ride_id,category,severity,title,description,participant_ids) VALUES ($2,($1->>'rideId')::uuid,$1->>'category',$1->>'severity',$1->>'title',$1->>'description',ARRAY(SELECT jsonb_array_elements_text($1->'participantIds')::uuid)) RETURNING jsonb_build_object('id',id) AS data",serde_json::to_value(p).unwrap()).await?;
    let mut db = connection(&s).await?;
    Ok((StatusCode::CREATED, Json(incident_data(&mut db, id).await?)))
}
#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct IncidentPatch {
    status: Option<String>,
    severity: Option<String>,
    assignee_id: Option<Uuid>,
}
#[utoipa::path(patch,path="/admin/incidents/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Incident),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=IncidentPatch,params(("id"=Uuid,Path)))]
async fn update_incident(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<IncidentPatch>,
) -> Result<Json<Value>, AdminError> {
    if let Some(ref v) = p.status {
        member(v, &["OPEN", "UNDER_REVIEW", "RESOLVED"])?;
    }
    if let Some(ref v) = p.severity {
        member(v, &["LOW", "MEDIUM", "HIGH", "CRITICAL"])?;
    }
    mutate(&s,&c,id,"incident.updated","UPDATE admin_incidents SET status=COALESCE($1->>'status',status), severity=COALESCE($1->>'severity',severity),assignee_id=COALESCE(($1->>'assigneeId')::uuid,assignee_id),updated_at=(now() AT TIME ZONE 'UTC'),resolved_at=CASE WHEN $1->>'status'='RESOLVED' THEN COALESCE(resolved_at,(now() AT TIME ZONE 'UTC')) WHEN $1->>'status' IN ('OPEN','UNDER_REVIEW') THEN NULL ELSE resolved_at END WHERE id=$2 RETURNING jsonb_build_object('id',id) AS data",serde_json::to_value(p).unwrap()).await?;
    let mut db = connection(&s).await?;
    Ok(Json(incident_data(&mut db, id).await?))
}
#[utoipa::path(post,path="/admin/incidents/{id}/notes",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
async fn add_note(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    let note = p.note.ok_or_else(|| bad("note is required"))?;
    text(&note)?;
    Ok(Json(mutate(&s,&c,id,"incident.note_added","INSERT INTO admin_incident_notes(id,incident_id,author_id,note) VALUES (($1->>'id')::uuid,$2,($1->>'author')::uuid,$1->>'note') RETURNING jsonb_build_object('id',id,'author',author_id,'note',note,'at',created_at AT TIME ZONE 'UTC') AS data",json!({"id":Uuid::now_v7(),"author":c.user_id,"note":note})).await?))
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct Call {
    participant_id: Uuid,
}
#[utoipa::path(post,path="/admin/incidents/{id}/calls",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),request_body=Call,params(("id"=Uuid,Path)),responses((status=200,description="Logged call initiation, with phone and tel URL",body=serde_json::Value),(status=404,description="Open incident or participant not found")))]
async fn log_call(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Call>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(mutate(&s,&c,id,"incident.call_started","SELECT jsonb_build_object('userId',u.id,'phone',u.phone,'callHref','tel:' || u.phone) AS data FROM admin_incidents i LEFT JOIN rides r ON r.id=i.ride_id JOIN users u ON u.id=($1->>'participantId')::uuid AND (u.id=ANY(i.participant_ids) OR u.id=r.passenger_id OR u.id=r.driver_id) WHERE i.id=$2 AND i.status<>'RESOLVED'",json!({"participantId":p.participant_id})).await?))
}

const PEOPLE_FILTER: &str = "($1->>'q' IS NULL OR concat_ws(' ',data->>'displayName',data->>'email',data->'vehicle'->>'plate') ILIKE '%' || ($1->>'q') || '%') AND ($1->>'track' IS NULL OR track=$1->>'track' OR track='Both') AND ($1->>'availability' IS NULL OR upper(data->>'availability')=upper($1->>'availability')) AND ($1->>'verification' IS NULL OR upper(verification_status)=upper($1->>'verification')) AND ($1->>'active' IS NULL OR is_active=($1->>'active')::boolean) AND ($1->>'flagged' IS NULL OR (data->>'flagged')::boolean=($1->>'flagged')::boolean) AND ($1->>'membership' IS NULL OR data->>'membershipStatus'=$1->>'membership') AND ($1->>'hasTrustedContact' IS NULL OR (COALESCE((data->>'trustedContactCount')::int,0)>0)=($1->>'hasTrustedContact')::boolean)";
async fn people(
    s: &AppState,
    c: &Claims,
    f: Filters,
    role: &str,
) -> Result<Json<Value>, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    let mut result = query(
        &mut db,
        &page_sql(
            "admin_people_view",
            &format!("role='{role}' AND {PEOPLE_FILTER}"),
        ),
        f.params()?,
        c.user_id,
    )
    .await?;
    result["counts"]=query(&mut db,"SELECT jsonb_build_object('all',count(*),'online',count(*) FILTER(WHERE data->>'availability'='ONLINE'),'pending',count(*) FILTER(WHERE COALESCE(upper(verification_status),'DRAFT')<>'APPROVED'),'flagged',count(*) FILTER(WHERE (data->>'flagged')::boolean),'deactivated',count(*) FILTER(WHERE NOT is_active),'verified',count(*) FILTER(WHERE data->>'membershipStatus'='VERIFIED'),'unverified',count(*) FILTER(WHERE data->>'membershipStatus'<>'VERIFIED'),'noTrustedContact',count(*) FILTER(WHERE (data->>'trustedContactCount')::int=0)) AS data FROM admin_people_view WHERE role=$1->>'role' AND $2 IS NOT NULL",json!({"role":role}),c.user_id).await?;
    Ok(Json(result))
}
#[utoipa::path(get,path="/admin/drivers",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::PersonSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("q"=Option<String>,Query),("track"=Option<String>,Query),("availability"=Option<String>,Query),("verification"=Option<String>,Query),("flagged"=Option<bool>,Query),("active"=Option<bool>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn drivers(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    people(&s, &c, f, "Driver").await
}
#[utoipa::path(get,path="/admin/passengers",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::PersonSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("q"=Option<String>,Query),("membership"=Option<String>,Query),("hasTrustedContact"=Option<bool>,Query),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn passengers(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    people(&s, &c, f, "User").await
}
async fn person(s: &AppState, c: &Claims, id: Uuid, role: &str) -> Result<Json<Value>, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    let mut v = query(
        &mut db,
        &format!(
            "SELECT data FROM admin_people_view WHERE id=$2 AND role='{role}' AND $1 IS NOT NULL"
        ),
        json!({}),
        id,
    )
    .await?;
    v["stats"]=query(&mut db,"SELECT jsonb_build_object('completedRides',count(*) FILTER(WHERE status='COMPLETED'),'weekRides',count(*) FILTER(WHERE status='COMPLETED' AND completed_at>=date_trunc('week',(now() AT TIME ZONE 'UTC'))),'weekEarnings',jsonb_build_object('amountMinor',COALESCE(sum(COALESCE(final_fare_minor,fare_total_minor+waiting_charge_minor)) FILTER(WHERE status='COMPLETED' AND completed_at>=date_trunc('week',(now() AT TIME ZONE 'UTC'))),0),'currency','NGN')) AS data FROM rides WHERE CASE WHEN $1->>'role'='Driver' THEN driver_id=$2 ELSE passenger_id=$2 END",json!({"role":role}),id).await?;
    if role == "Driver" {
        v["affiliation"]=query(&mut db,"SELECT jsonb_build_object('isKoinoniaWorker',is_koinonia_worker,'department',koinonia_department,'guarantor',guarantor_name) AS data FROM driver_profiles WHERE user_id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
        v["trackSwitch"] = Value::Null;
    }
    v["stats"]["rating"] = v["rating"].clone();
    v["trustedContacts"]=query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'name',name,'relationship',relationship)),'[]'::jsonb) AS data FROM trusted_contacts WHERE user_id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    Ok(Json(v))
}
#[utoipa::path(get,path="/admin/drivers/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn driver(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    person(&s, &c, id, "Driver").await
}
#[utoipa::path(get,path="/admin/passengers/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn passenger(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    person(&s, &c, id, "User").await
}
async fn history(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    f: Filters,
    column: &str,
) -> Result<Json<Value>, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    Ok(Json(
        query(
            &mut db,
            &page_sql("admin_rides_view", &format!("{column}=$2")),
            f.params()?,
            id,
        )
        .await?,
    ))
}
#[utoipa::path(get,path="/admin/drivers/{id}/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn driver_rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    history(&s, &c, id, f, "driver_id").await
}
#[utoipa::path(get,path="/admin/passengers/{id}/rides",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=dto::Page<dto::AdminRideSummary>),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path),("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
async fn passenger_rides(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    history(&s, &c, id, f, "passenger_id").await
}
#[derive(Deserialize, utoipa::ToSchema)]
struct Reason {
    reason: String,
}
#[utoipa::path(post,path="/admin/drivers/{id}/flag",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
async fn flag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    text(&p.reason)?;
    Ok(Json(mutate(&s,&c,id,"quality.flagged","INSERT INTO admin_quality_flags(id,subject_id,reason) SELECT ($1->>'id')::uuid,id,$1->>'reason' FROM users WHERE id=$2 AND role='Driver' ON CONFLICT(subject_id) WHERE cleared_at IS NULL DO UPDATE SET reason=EXCLUDED.reason RETURNING jsonb_build_object('id',id,'reason',reason) AS data",json!({"id":Uuid::now_v7(),"reason":p.reason})).await?))
}
#[utoipa::path(delete,path="/admin/drivers/{id}/flag",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn unflag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(mutate(&s,&c,id,"quality.cleared","UPDATE admin_quality_flags SET cleared_at=(now() AT TIME ZONE 'UTC') WHERE subject_id=$2 AND cleared_at IS NULL AND $1 IS NOT NULL RETURNING jsonb_build_object('id',id) AS data",json!({})).await?))
}
#[utoipa::path(post,path="/admin/drivers/{id}/deactivate",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
async fn deactivate(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    text(&p.reason)?;
    let v=mutate(&s,&c,id,"driver.deactivated","WITH u AS (UPDATE users SET is_active=false,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 AND role='Driver' RETURNING id), st AS (INSERT INTO admin_user_state(user_id,deactivation_reason) SELECT id,$1->>'reason' FROM u ON CONFLICT(user_id) DO UPDATE SET deactivation_reason=EXCLUDED.deactivation_reason,updated_at=(now() AT TIME ZONE 'UTC')), av AS (UPDATE driver_availability SET status='OFFLINE',updated_at=(now() AT TIME ZONE 'UTC') WHERE driver_id IN (SELECT id FROM u)), offers AS (UPDATE ride_offers SET status='withdrawn' WHERE driver_id IN(SELECT id FROM u) AND status='offered') SELECT jsonb_build_object('userId',id,'accountStatus','DEACTIVATED') AS data FROM u",json!({"reason":p.reason})).await?;
    s.ws_registry.set_driver_online(id, false);
    let mut dispatcher = (*s.dispatcher).clone();
    dispatcher
        .remove_driver(&id.to_string())
        .await
        .map_err(|e| {
            tracing::error!(error=%e,"Failed to remove deactivated driver from dispatch index");
            AdminError(
                StatusCode::SERVICE_UNAVAILABLE,
                "dispatch_unavailable",
                "Account deactivated; dispatch cleanup failed".into(),
            )
        })?;
    Ok(Json(v))
}
#[utoipa::path(post,path="/admin/passengers/{id}/restriction",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Reason,params(("id"=Uuid,Path)))]
async fn restrict(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Reason>,
) -> Result<Json<Value>, AdminError> {
    text(&p.reason)?;
    restriction(&s, &c, id, true, Some(p.reason)).await
}
#[utoipa::path(delete,path="/admin/passengers/{id}/restriction",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn unrestrict(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    restriction(&s, &c, id, false, None).await
}
async fn restriction(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    value: bool,
    reason: Option<String>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(mutate(s,c,id,"passenger.restriction_changed","INSERT INTO admin_user_state(user_id,restricted,restriction_reason) SELECT id,($1->>'restricted')::boolean,$1->>'reason' FROM users WHERE id=$2 AND role='User' ON CONFLICT(user_id) DO UPDATE SET restricted=EXCLUDED.restricted,restriction_reason=EXCLUDED.restriction_reason,updated_at=(now() AT TIME ZONE 'UTC') RETURNING jsonb_build_object('userId',user_id,'restricted',restricted) AS data",json!({"restricted":value,"reason":reason})).await?))
}
#[utoipa::path(post,path="/admin/passengers/{id}/membership-check",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
async fn membership_check(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(mutate(&s,&c,id,"membership.check_requested","INSERT INTO admin_user_state(user_id,membership_status) SELECT id,'CHECKING' FROM users WHERE id=$2 AND role='User' AND $1 IS NOT NULL ON CONFLICT(user_id) DO UPDATE SET membership_status='CHECKING',updated_at=(now() AT TIME ZONE 'UTC') RETURNING jsonb_build_object('userId',user_id,'membershipStatus',membership_status) AS data",json!({})).await?))
}
#[utoipa::path(get,path="/admin/quality-flags",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("role"=Option<String>,Query),("status"=Option<String>,Query)))]
async fn quality(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(Json(query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',f.id,'subject',p.data-'email'-'homeArea'-'inspection'-'deactivationReason','averageRating',p.data->'rating','lowRatings',p.data->'lowRatings','rides',p.data->'completedRides','reason',f.reason,'source',f.source,'flaggedAt',f.flagged_at AT TIME ZONE 'UTC') ORDER BY f.flagged_at DESC),'[]'::jsonb) AS data FROM admin_quality_flags f JOIN admin_people_view p ON p.id=f.subject_id WHERE ($1->>'role' IS NULL OR p.role=$1->>'role') AND (CASE WHEN COALESCE($1->>'status','open')='open' THEN f.cleared_at IS NULL ELSE f.cleared_at IS NOT NULL END) AND $2 IS NOT NULL",f.params()?,c.user_id).await?))
}
#[utoipa::path(post,path="/admin/quality-flags/{id}/clear",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Note,params(("id"=Uuid,Path)))]
async fn clear_flag(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
    Json(p): Json<Note>,
) -> Result<Json<Value>, AdminError> {
    Ok(Json(mutate(&s,&c,id,"quality.cleared","UPDATE admin_quality_flags SET cleared_at=(now() AT TIME ZONE 'UTC'),clear_note=$1->>'note' WHERE id=$2 AND cleared_at IS NULL RETURNING jsonb_build_object('id',id,'clearedAt',cleared_at AT TIME ZONE 'UTC') AS data",json!({"note":p.note})).await?))
}

pub async fn requests_restricted(db: &mut Connection<'_>, id: Uuid) -> Result<bool, ModuleError> {
    let row=diesel::sql_query("SELECT jsonb_build_object('restricted',COALESCE((SELECT restricted FROM admin_user_state WHERE user_id=$1),false)) AS data").bind::<SqlUuid,_>(id).get_result::<Row>(db).await?;
    Ok(row.data["restricted"].as_bool().unwrap_or(false))
}

#[utoipa::path(get,path="/rides/me/notifications",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("cursor"=Option<Uuid>,Query),("limit"=Option<i64>,Query)))]
pub async fn notifications(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Query(f): Query<Filters>,
) -> Result<Json<Value>, AdminError> {
    let mut db = connection(&s).await?;
    Ok(Json(query(&mut db,&page_sql("(SELECT id,recipient_id,jsonb_build_object('id',id,'type',event_type,'payload',payload,'createdAt',created_at AT TIME ZONE 'UTC') AS data FROM ride_notifications) notifications","recipient_id=$2"),f.params()?,c.user_id).await?))
}
#[derive(Deserialize, utoipa::ToSchema)]
pub struct Contact {
    name: String,
    relationship: String,
    phone: String,
}
#[utoipa::path(get,path="/rides/me/trusted-contacts",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")))]
pub async fn contacts(
    State(s): State<Arc<AppState>>,
    c: Claims,
) -> Result<Json<Value>, AdminError> {
    let mut db = connection(&s).await?;
    Ok(Json(query(&mut db,"SELECT COALESCE(jsonb_agg(jsonb_build_object('id',id,'name',name,'relationship',relationship,'phone',phone)),'[]'::jsonb) AS data FROM trusted_contacts WHERE user_id=$2 AND $1 IS NOT NULL",json!({}),c.user_id).await?))
}
#[utoipa::path(post,path="/rides/me/trusted-contacts",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=201,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),request_body=Contact)]
pub async fn add_contact(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Json(p): Json<Contact>,
) -> Result<(StatusCode, Json<Value>), AdminError> {
    text(&p.name)?;
    text(&p.relationship)?;
    text(&p.phone)?;
    let mut db = connection(&s).await?;
    Ok((StatusCode::CREATED,Json(query(&mut db,"INSERT INTO trusted_contacts(id,user_id,name,relationship,phone) VALUES(($1->>'id')::uuid,$2,$1->>'name',$1->>'relationship',$1->>'phone') RETURNING jsonb_build_object('id',id,'name',name,'relationship',relationship,'phone',phone) AS data",json!({"id":Uuid::now_v7(),"name":p.name,"relationship":p.relationship,"phone":p.phone}),c.user_id).await?)))
}
#[utoipa::path(delete,path="/rides/me/trusted-contacts/{id}",tag="Paid Rides Admin",security(("bearerAuth"=[]),("cookieAuth"=[])),responses((status=200,description="Success",body=serde_json::Value),(status=400,description="Invalid request"),(status=403,description="Admin or Root required"),(status=404,description="Not found"),(status=409,description="Workflow conflict")),params(("id"=Uuid,Path)))]
pub async fn remove_contact(
    State(s): State<Arc<AppState>>,
    c: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AdminError> {
    let mut db = connection(&s).await?;
    Ok(Json(query(&mut db,"DELETE FROM trusted_contacts WHERE id=($1->>'id')::uuid AND user_id=$2 RETURNING jsonb_build_object('id',id) AS data",json!({"id":id}),c.user_id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn role_and_paging_contracts() {
        for role in [
            crate::users::RoleType::User,
            crate::users::RoleType::Driver,
            crate::users::RoleType::RouteCoordinator,
        ] {
            assert!(
                require_admin(&Claims {
                    user_id: Uuid::now_v7(),
                    role,
                    exp: usize::MAX
                })
                .is_err()
            );
        }
        for role in [crate::users::RoleType::Admin, crate::users::RoleType::Root] {
            assert!(
                require_admin(&Claims {
                    user_id: Uuid::now_v7(),
                    role,
                    exp: usize::MAX
                })
                .is_ok()
            );
        }
        assert_eq!(Filters::default().params().unwrap()["limit"], 25);
        assert!(
            Filters {
                limit: Some(101),
                ..Default::default()
            }
            .params()
            .is_err()
        );
        assert!(
            Filters {
                limit: Some(0),
                ..Default::default()
            }
            .params()
            .is_err()
        );
    }
    /// Explicitly opt in with a disposable database that has all migrations applied.
    #[tokio::test]
    #[ignore = "requires ADMIN_TEST_DATABASE_URL pointing to a migrated disposable database"]
    async fn database_projection_and_safety_contracts() {
        use diesel_async::SimpleAsyncConnection;
        let url = std::env::var("ADMIN_TEST_DATABASE_URL").expect("ADMIN_TEST_DATABASE_URL");
        let manager = crate::AsyncDieselConnectionManager::<crate::AsyncPgConnection>::new(url);
        let pool = bb8::Pool::builder()
            .max_size(1)
            .build(manager)
            .await
            .unwrap();
        let mut db = pool.get().await.unwrap();
        db.batch_execute("BEGIN").await.unwrap();
        let admin = Uuid::now_v7();
        let driver = Uuid::now_v7();
        let passenger = Uuid::now_v7();
        let car = Uuid::now_v7();
        let ride = Uuid::now_v7();
        let alert = Uuid::now_v7();
        for (id, role) in [(admin, "Admin"), (driver, "Driver"), (passenger, "User")] {
            query(&mut db,"INSERT INTO users(id,first_name,last_name,email,username,phone,address,country,role,city,password_hash) VALUES($2,'Test','Person',($2::text)||'@test.invalid',$2::text,$2::text,'address','NG',$1->>'role','Test','unused') RETURNING '{}'::jsonb AS data",json!({"role":role}),id).await.unwrap();
        }
        query(&mut db,"INSERT INTO cars(id,license_plate,capacity,car_type,car_tier,car_size,color,brand,model,year,vehicle_photo,vehicle_registration,insurance_certificate,roadworthiness_certificate) VALUES($2,'ADMIN-TEST',4,'sedan','standard','medium','blue','Test','Car',2026,'body','body','body','body') RETURNING '{}'::jsonb AS data",json!({}),car).await.unwrap();
        query(&mut db,"INSERT INTO driver_profiles(id,user_id,car_id,nin_slip,profile_photo,is_koinonia_worker,licence_number,licence_class,licence_expiry_date,licence_photo,track,status) VALUES($2,$2,($1->>'car')::uuid,'body','body',true,'test','B',CURRENT_DATE+365,'body','Professional','Pending') RETURNING '{}'::jsonb AS data",json!({"car":car}),driver).await.unwrap();
        let docs = query(&mut db, applications::DOCUMENT_SQL, json!({}), driver)
            .await
            .unwrap();
        assert_eq!(docs.as_array().unwrap().len(), 7);
        assert!(
            docs.as_array()
                .unwrap()
                .iter()
                .all(|d| d["status"] == "UPLOADED")
        );
        let claims = Claims {
            user_id: admin,
            role: crate::users::RoleType::Admin,
            exp: usize::MAX,
        };
        let approval: applications::Decision =
            serde_json::from_value(json!({"decision":"APPROVE"})).unwrap();
        let blocked = applications::apply_decision(&mut db, &claims, driver, &approval)
            .await
            .unwrap_err();
        assert_eq!(blocked.0, StatusCode::CONFLICT);
        for d in docs.as_array().unwrap() {
            query(&mut db,"INSERT INTO admin_document_reviews(user_id,document_type,status,reviewer_id) VALUES($2,$1->>'doc','VERIFIED',($1->>'reviewer')::uuid) RETURNING '{}'::jsonb AS data",json!({"doc":d["type"],"reviewer":admin}),driver).await.unwrap();
        }
        let approved = applications::apply_decision(&mut db, &claims, driver, &approval)
            .await
            .unwrap();
        assert_eq!(approved["status"], "APPROVED");
        let car_status=query(&mut db,"SELECT jsonb_build_object('verified',is_verified) AS data FROM cars WHERE id=$2 AND $1 IS NOT NULL",json!({}),car).await.unwrap();
        assert_eq!(car_status["verified"], true);
        query(&mut db,"UPDATE driver_profiles SET licence_photo='replacement',status='SUBMITTED' WHERE user_id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),driver).await.unwrap();
        let blocked = applications::apply_decision(&mut db, &claims, driver, &approval)
            .await
            .unwrap_err();
        assert_eq!(blocked.0, StatusCode::CONFLICT);
        let people = query(
            &mut db,
            &page_sql("admin_people_view", "role='Driver'"),
            Filters {
                limit: Some(1),
                ..Default::default()
            }
            .params()
            .unwrap(),
            admin,
        )
        .await
        .unwrap();
        assert_eq!(people["items"][0]["verificationStatus"], "SUBMITTED");
        serde_json::from_value::<dto::Page<dto::PersonSummary>>(people.clone()).unwrap();
        assert!(!people.to_string().contains("base64"));
        query(&mut db,"INSERT INTO rides(id,passenger_id,driver_id,status,option_id,option_snapshot,estimate_id,pickup,destination,route,fare,currency,fare_total_minor,trip_pin) VALUES($2,($1->>'passenger')::uuid,($1->>'driver')::uuid,'IN_PROGRESS','go','{}',$2,'{\"label\":\"Pickup\"}','{\"label\":\"Destination\"}','{}','{}','NGN',100000,'1234') RETURNING '{}'::jsonb AS data",json!({"passenger":passenger,"driver":driver}),ride).await.unwrap();
        query(&mut db,"INSERT INTO safety_events(id,ride_id,reporter_id,reporter_role,kind,event_type,status,occurred_at) VALUES($2,($1->>'ride')::uuid,($1->>'passenger')::uuid,'passenger','sos','threat','sent',(now() AT TIME ZONE 'UTC')) RETURNING '{}'::jsonb AS data",json!({"ride":ride,"passenger":passenger}),alert).await.unwrap();
        let open = incident_data(&mut db, alert).await.unwrap();
        serde_json::from_value::<dto::Incident>(open.clone()).unwrap();
        assert_eq!(open["category"], "SOS_ACTIVATION");
        assert_eq!(open["severity"], "CRITICAL");
        assert_eq!(open["participants"].as_array().unwrap().len(), 2);
        assert!(
            open["participants"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p.get("phone").is_some())
        );
        query(&mut db,"UPDATE admin_incidents SET status='RESOLVED' WHERE id=$2 AND $1 IS NOT NULL RETURNING '{}'::jsonb AS data",json!({}),alert).await.unwrap();
        let closed = incident_data(&mut db, alert).await.unwrap();
        assert!(
            closed["participants"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p.get("phone").is_none())
        );
        let notifications=query(&mut db,"SELECT jsonb_build_object('n',count(*)) AS data FROM ride_notifications WHERE recipient_id IN(($1->>'driver')::uuid,$2)",json!({"driver":driver}),passenger).await.unwrap();
        assert_eq!(notifications["n"], 2);
        let report = query(&mut db, reports::REPORT_SQL, json!({"days":7}), admin)
            .await
            .unwrap();
        assert_eq!(report.as_array().unwrap().len(), 6);
        let series = query(&mut db, reports::SERIES_SQL, json!({"days":7}), admin)
            .await
            .unwrap();
        assert_eq!(series["adoption"].as_array().unwrap().len(), 7);
        let page = query(
            &mut db,
            &page_sql("admin_people_view", "true"),
            Filters {
                limit: Some(1),
                ..Default::default()
            }
            .params()
            .unwrap(),
            admin,
        )
        .await
        .unwrap();
        let cursor = serde_json::from_value(page["nextCursor"].clone()).unwrap();
        let next = query(
            &mut db,
            &page_sql("admin_people_view", "true"),
            Filters {
                limit: Some(1),
                cursor: Some(cursor),
                ..Default::default()
            }
            .params()
            .unwrap(),
            admin,
        )
        .await
        .unwrap();
        assert_ne!(page["items"][0]["userId"], next["items"][0]["userId"]);
        query(&mut db,"INSERT INTO admin_user_state(user_id,restricted,restriction_reason) VALUES($2,true,COALESCE($1->>'reason','test')) ON CONFLICT(user_id) DO UPDATE SET restricted=true RETURNING '{}'::jsonb AS data",json!({}),passenger).await.unwrap();
        assert!(requests_restricted(&mut db, passenger).await.unwrap());

        audit(&mut db, admin, driver, "verification.test", json!({}))
            .await
            .unwrap();
        db.batch_execute("ROLLBACK").await.unwrap();
    }
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(log_call,rides,ride,sos,acknowledge,resolve,incidents,incident,create_incident,update_incident,add_note,drivers,driver,driver_rides,flag,unflag,deactivate,passengers,passenger,passenger_rides,restrict,unrestrict,membership_check,quality,clear_flag,notifications,contacts,add_contact,remove_contact,applications::list,applications::detail,applications::document,applications::comment,applications::inspection,applications::driver_inspection,applications::decision,applications::own_review,reports::overview,reports::reports,reports::export),modifiers(&AdminSecurity))]
pub struct AdminApiDoc;
struct AdminSecurity;
impl utoipa::Modify for AdminSecurity {
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::*;
        let components = doc.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
        components.add_security_scheme(
            "cookieAuth",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("access_token"))),
        );
    }
}
