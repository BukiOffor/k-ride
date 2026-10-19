//! Business logic and database operations for safety.
use super::*;

pub(crate) const CATEGORIES: &[&str] = &[
    "SAFETY_CONCERN",
    "SOS_ACTIVATION",
    "DRIVER_CONDUCT",
    "PASSENGER_CONDUCT",
    "VEHICLE_ISSUE",
    "PAYMENT_DISPUTE",
    "OTHER",
];

/// List SOS alerts, optionally restricted to active alerts.
pub async fn sos(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(query(&mut db,"SELECT COALESCE(jsonb_agg(data ORDER BY created_at DESC,id DESC),'[]'::jsonb) AS data FROM admin_sos_view WHERE ($1->>'status' IS NULL OR ($1->>'status'='active' AND status IN ('sent','acknowledged','responding')) OR status=$1->>'status') AND $2 IS NOT NULL",f.params()?,c.user_id).await?)
}

/// Acknowledge an SOS and synchronize the ride safety state.
pub async fn acknowledge(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    mutate(&s,&c,id,"sos.acknowledged","WITH e AS (UPDATE safety_events SET status=CASE WHEN status='sent' THEN 'acknowledged' ELSE status END WHERE id=$2 AND kind='sos' AND $1 IS NOT NULL RETURNING *), r AS (UPDATE rides SET sos_status=CASE WHEN rides.sos_status='sent' THEN 'acknowledged' ELSE rides.sos_status END WHERE id IN(SELECT ride_id FROM e)) SELECT jsonb_build_object('id',id) AS data FROM e",json!({})).await?;
    let mut db = connection(&s).await?;
    Ok(query(
        &mut db,
        "SELECT data FROM admin_sos_view WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        id,
    )
    .await?)
}

/// Resolve an SOS while keeping its incident available for further review.
pub async fn resolve(s: Arc<AppState>, c: Claims, id: Uuid, n: Note) -> Result<Value, AdminError> {
    mutate(&s,&c,id,"sos.resolved","WITH e AS (UPDATE safety_events SET status='resolved' WHERE id=$2 AND kind='sos' RETURNING *), r AS (UPDATE rides SET sos_status=CASE WHEN EXISTS(SELECT 1 FROM safety_events other WHERE other.ride_id=rides.id AND other.kind='sos' AND other.id<>$2 AND other.status<>'resolved') THEN 'sent' ELSE 'resolved' END WHERE id IN (SELECT ride_id FROM e)) SELECT jsonb_build_object('id',id,'status',status) AS data FROM e WHERE $1 IS NOT NULL",json!({"note":n.note})).await?;
    let mut db = connection(&s).await?;
    Ok(query(
        &mut db,
        "SELECT data FROM admin_sos_view WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        id,
    )
    .await?)
}

/// List incidents with filters, pagination and status counts.
pub async fn incidents(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let sql = page_sql(
        "admin_incidents_view",
        "($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'severity' IS NULL OR severity=$1->>'severity') AND ($1->>'category' IS NULL OR category=$1->>'category') AND ($1->>'q' IS NULL OR concat_ws(' ',data->>'reference',data->>'title') ILIKE '%' || ($1->>'q') || '%')",
    );
    let mut v = query(&mut db, &sql, f.params()?, c.user_id).await?;
    v["counts"]=query(&mut db,"SELECT jsonb_build_object('all',count(*),'open',count(*) FILTER(WHERE status='OPEN'),'underReview',count(*) FILTER(WHERE status='UNDER_REVIEW'),'resolved',count(*) FILTER(WHERE status='RESOLVED'),'high',count(*) FILTER(WHERE severity IN ('HIGH','CRITICAL'))) AS data FROM admin_incidents WHERE $1 IS NOT NULL AND $2 IS NOT NULL",json!({}),c.user_id).await?;
    Ok(v)
}

/// Load an incident, participants, timeline and reviewer notes.
pub async fn incident(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    Ok(incident_data(&mut db, id).await?)
}

/// Validate and open a manual incident, recording an audit event.
pub async fn create_incident(
    s: Arc<AppState>,
    c: Claims,
    p: NewIncident,
) -> Result<Value, AdminError> {
    member(&p.category, CATEGORIES)?;
    member(&p.severity, &["LOW", "MEDIUM", "HIGH", "CRITICAL"])?;
    text(&p.title)?;
    text(&p.description)?;
    let id = Uuid::now_v7();
    mutate(&s,&c,id,"incident.opened","INSERT INTO admin_incidents(id,ride_id,category,severity,title,description,participant_ids) VALUES ($2,($1->>'rideId')::uuid,$1->>'category',$1->>'severity',$1->>'title',$1->>'description',ARRAY(SELECT jsonb_array_elements_text($1->'participantIds')::uuid)) RETURNING jsonb_build_object('id',id) AS data",serde_json::to_value(p).unwrap()).await?;
    let mut db = connection(&s).await?;
    Ok(incident_data(&mut db, id).await?)
}

/// Update incident status, severity or assignment and return refreshed detail.
pub async fn update_incident(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: IncidentPatch,
) -> Result<Value, AdminError> {
    if let Some(ref v) = p.status {
        member(v, &["OPEN", "UNDER_REVIEW", "RESOLVED"])?;
    }
    if let Some(ref v) = p.severity {
        member(v, &["LOW", "MEDIUM", "HIGH", "CRITICAL"])?;
    }
    mutate(&s,&c,id,"incident.updated","UPDATE admin_incidents SET status=COALESCE($1->>'status',status), severity=COALESCE($1->>'severity',severity),assignee_id=COALESCE(($1->>'assigneeId')::uuid,assignee_id),updated_at=(now() AT TIME ZONE 'UTC'),resolved_at=CASE WHEN $1->>'status'='RESOLVED' THEN COALESCE(resolved_at,(now() AT TIME ZONE 'UTC')) WHEN $1->>'status' IN ('OPEN','UNDER_REVIEW') THEN NULL ELSE resolved_at END WHERE id=$2 RETURNING jsonb_build_object('id',id) AS data",serde_json::to_value(p).unwrap()).await?;
    let mut db = connection(&s).await?;
    Ok(incident_data(&mut db, id).await?)
}

/// Append an attributed admin note to an incident and record the action.
pub async fn add_note(s: Arc<AppState>, c: Claims, id: Uuid, p: Note) -> Result<Value, AdminError> {
    let note = p.note.ok_or_else(|| bad("note is required"))?;
    text(&note)?;
    Ok(mutate(&s,&c,id,"incident.note_added","INSERT INTO admin_incident_notes(id,incident_id,author_id,note) VALUES (($1->>'id')::uuid,$2,($1->>'author')::uuid,$1->>'note') RETURNING jsonb_build_object('id',id,'author',author_id,'note',note,'at',created_at AT TIME ZONE 'UTC') AS data",json!({"id":Uuid::now_v7(),"author":c.user_id,"note":note})).await?)
}

/// Log a call initiation and release the selected open-incident participant’s tel URL.
pub async fn log_call(s: Arc<AppState>, c: Claims, id: Uuid, p: Call) -> Result<Value, AdminError> {
    Ok(mutate(&s,&c,id,"incident.call_started","SELECT jsonb_build_object('userId',u.id,'phone',u.phone,'callHref','tel:' || u.phone) AS data FROM admin_incidents i LEFT JOIN rides r ON r.id=i.ride_id JOIN users u ON u.id=($1->>'participantId')::uuid AND (u.id=ANY(i.participant_ids) OR u.id=r.passenger_id OR u.id=r.driver_id) WHERE i.id=$2 AND i.status<>'RESOLVED'",json!({"participantId":p.participant_id})).await?)
}

/// Build incident detail and release phone numbers only while it is open.
pub(crate) async fn incident_data(db: &mut Connection<'_>, id: Uuid) -> Result<Value, AdminError> {
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
