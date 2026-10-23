//! Scheduled bookings: fare snapshots, serialized acceptance and reusable reservation locks.
use crate::vrp::{dtos::scheduled::*, models::scheduled::ScheduledJsonRow};
use crate::{AppState, Connection, ModuleError, auth::jwt::Claims};
use chrono::{DateTime, Utc};
use diesel::{
    OptionalExtension,
    sql_types::{Jsonb, Uuid as SqlUuid},
};
use diesel_async::{AsyncConnection, RunQueryDsl};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

/// Convert the environment buffer setting to a bounded value; invalid values retain the 30-minute policy.
pub fn buffer_minutes() -> i32 {
    std::env::var("SCHEDULED_RIDE_BUFFER_MINUTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| (0..=240).contains(n))
        .unwrap_or(30)
}
/// Validate a future pickup within the initial one-hour to thirty-day booking window.
pub fn validate_pickup(at: DateTime<Utc>, now: DateTime<Utc>) -> Result<(), ModuleError> {
    if at < now + chrono::Duration::hours(1) || at > now + chrono::Duration::days(30) {
        return Err(ModuleError::BadRequest(
            "Schedule between one hour and thirty days ahead",
        ));
    }
    Ok(())
}
/// Execute a parameterized projection, using JSON for validated inputs and a separate resource UUID.
pub(crate) async fn query(
    db: &mut Connection<'_>,
    sql: &str,
    p: Value,
    id: Uuid,
) -> Result<Value, ModuleError> {
    Ok(diesel::sql_query(sql)
        .bind::<Jsonb, _>(p)
        .bind::<SqlUuid, _>(id)
        .get_result::<ScheduledJsonRow>(db)
        .await?
        .data)
}
/// Acquire an existing pool connection without changing transaction ownership.
pub(crate) async fn connection(s: &AppState) -> Result<Connection<'_>, ModuleError> {
    s.pool
        .get()
        .await
        .map_err(|e| ModuleError::InternalError(e.to_string().into()))
}
/// Require a current active account, and reject restricted passengers at booking time.
async fn passenger_allowed(db: &mut Connection<'_>, id: Uuid) -> Result<(), ModuleError> {
    let v=query(db,"SELECT jsonb_build_object('allowed',is_active AND role IN ('User','Driver') AND NOT COALESCE((SELECT restricted FROM admin_user_state WHERE user_id=$2),false)) AS data FROM users WHERE id=$2 AND $1 IS NOT NULL",json!({}),id).await?;
    if v["allowed"] != true {
        return Err(ModuleError::PermissionDenied);
    }
    Ok(())
}
/// Validate a named timezone in PostgreSQL's installed timezone database.
async fn timezone_allowed(db: &mut Connection<'_>, zone: &str) -> Result<(), ModuleError> {
    let v=query(db,"SELECT jsonb_build_object('valid',EXISTS(SELECT 1 FROM pg_timezone_names WHERE name=$1->>'timezone')) AS data WHERE $2 IS NOT NULL",json!({"timezone":zone}),Uuid::nil()).await?;
    if v["valid"] != true {
        return Err(ModuleError::BadRequest("Unknown IANA timezone"));
    }
    Ok(())
}
/// Serialize commitments for the same passenger and driver, always taking user locks in UUID order.
pub(crate) async fn lock_users(
    db: &mut Connection<'_>,
    passenger: Uuid,
    driver: Option<Uuid>,
) -> Result<(), ModuleError> {
    query(db,"WITH locked AS (SELECT id FROM users WHERE id=$2 OR id=($1->>'driver')::uuid ORDER BY id FOR UPDATE) SELECT COALESCE(jsonb_agg(id),'[]'::jsonb) AS data FROM locked",json!({"driver":driver}),passenger).await?;
    Ok(())
}
/// Save an immutable server-priced journey; a database trigger writes the initial broadcast and emails atomically.
pub async fn create(s: Arc<AppState>, c: Claims, p: ScheduleRideDto) -> Result<Value, ModuleError> {
    let mut db = connection(&s).await?;
    let value=db.transaction::<Value,ModuleError,_>(|db|Box::pin(async {
  lock_users(db,c.user_id,None).await?;passenger_allowed(db,c.user_id).await?;timezone_allowed(db,&p.timezone).await?;
  let prior=diesel::sql_query("SELECT data || jsonb_build_object('_matches',scheduled_at=(($1->>'at')::timestamptz AT TIME ZONE 'UTC') AND timezone=$1->>'timezone' AND option_id=$1->>'option') AS data FROM scheduled_ride_view WHERE passenger_id=$2 AND estimate_id=($1->>'estimate')::uuid").bind::<Jsonb,_>(json!({"at":p.scheduled_at,"timezone":p.timezone,"option":p.option_id,"estimate":p.estimate_id})).bind::<SqlUuid,_>(c.user_id).get_result::<ScheduledJsonRow>(db).await.optional()?;
  if let Some(mut prior)=prior {let same=prior.data.as_object_mut().unwrap().remove("_matches")==Some(json!(true));if same{return Ok(prior.data);}return Err(ModuleError::Conflict("Estimate already used by another scheduled booking"));}
  validate_pickup(p.scheduled_at,Utc::now())?;
  let params=json!({"estimate":p.estimate_id,"option":p.option_id,"scheduledAt":p.scheduled_at,"timezone":p.timezone,"buffer":buffer_minutes(),"id":Uuid::now_v7()});
  let v=query(db,r#"WITH estimate AS (SELECT e.*,o.id AS option_id,to_jsonb(o) AS option_snapshot,q->'fare' AS quote FROM fare_estimates e JOIN ride_options o ON o.id=$1->>'option' CROSS JOIN LATERAL jsonb_array_elements(e.options) q WHERE e.id=($1->>'estimate')::uuid AND e.user_id=$2 AND e.expires_at>(now() AT TIME ZONE 'UTC') AND o.is_active AND NOT o.shared AND e.route->>'source'='road' AND (e.route->>'distanceMeters')::bigint>0 AND (e.route->>'durationSeconds')::bigint>0 AND q->'option'->>'id'=o.id), inserted AS (
  INSERT INTO scheduled_rides(id,passenger_id,estimate_id,option_id,option_snapshot,pickup,destination,route,fare,currency,fare_total_minor,scheduled_at,timezone,duration_seconds,buffer_minutes,next_broadcast_at)
  SELECT ($1->>'id')::uuid,$2,id,option_id,option_snapshot,pickup,destination,route,quote || jsonb_build_object('isEstimate',false,'range',jsonb_build_object('low',quote->'total','high',quote->'total'),'waiting',(quote->'waiting') || jsonb_build_object('perMinute',jsonb_build_object('amountMinor',0,'currency',quote->'total'->>'currency'))),quote->'total'->>'currency',(quote->'total'->>'amountMinor')::bigint,($1->>'scheduledAt')::timestamptz AT TIME ZONE 'UTC',$1->>'timezone',GREATEST(COALESCE((route->>'durationSeconds')::int,3600),60),($1->>'buffer')::int,($1->>'scheduledAt')::timestamptz AT TIME ZONE 'UTC'-interval '30 minutes' FROM estimate RETURNING id)
  SELECT jsonb_build_object('id',id) AS data FROM inserted"#,params.clone(),c.user_id).await.map_err(|e|if matches!(e,ModuleError::DieselError(diesel::result::Error::NotFound)){ModuleError::BadRequest("Invalid, expired or unavailable private-ride estimate")}else{e})?;
  let id=serde_json::from_value(v["id"].clone())?;
  ensure_no_overlap(db,id,c.user_id,None).await?;
  detail_query(db,id).await
 })).await?;
    wake(&s);
    Ok(value)
}
/// Load a booking while the caller already holds its required authorization or transaction locks.
pub(crate) async fn detail_query(db: &mut Connection<'_>, id: Uuid) -> Result<Value, ModuleError> {
    query(
        db,
        "SELECT data FROM scheduled_ride_view WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        id,
    )
    .await
}
/// Reject overlapping passenger reservations, driver reservations or live trips under the user locks.
pub(crate) async fn ensure_no_overlap(
    db: &mut Connection<'_>,
    id: Uuid,
    passenger: Uuid,
    driver: Option<Uuid>,
) -> Result<(), ModuleError> {
    let v=query(db,r#"SELECT jsonb_build_object('conflict',EXISTS(
 SELECT 1 FROM scheduled_rides x JOIN scheduled_rides s ON s.id=$2 WHERE x.id<>s.id AND x.status IN ('OPEN','RESERVED','ACTIVATED') AND (x.passenger_id=($1->>'passenger')::uuid OR x.driver_id=($1->>'driver')::uuid)
 AND x.scheduled_at<s.scheduled_at+make_interval(secs=>s.duration_seconds,mins=>s.buffer_minutes) AND x.scheduled_at+make_interval(secs=>x.duration_seconds,mins=>x.buffer_minutes)>s.scheduled_at
 UNION ALL SELECT 1 FROM rides r JOIN scheduled_rides s ON s.id=$2 WHERE r.status NOT IN ('COMPLETED','CANCELLED') AND r.id IS DISTINCT FROM s.ride_id AND (r.passenger_id=($1->>'passenger')::uuid OR r.driver_id=($1->>'driver')::uuid)
 AND s.scheduled_at<GREATEST((now() AT TIME ZONE 'UTC'),COALESCE(r.started_at,(now() AT TIME ZONE 'UTC'))+make_interval(secs=>GREATEST(COALESCE((r.route->>'durationSeconds')::int,3600),60)))+make_interval(mins=>s.buffer_minutes))) AS data"#,json!({"passenger":passenger,"driver":driver}),id).await?;
    if v["conflict"] == true {
        return Err(ModuleError::Conflict(
            "Journey overlaps another trip or its reservation buffer",
        ));
    }
    Ok(())
}
/// Page through a passenger's bookings or an eligible driver's board/reservations without releasing contact details.
pub async fn list(
    s: Arc<AppState>,
    c: Claims,
    f: ScheduledQuery,
    driver_board: bool,
) -> Result<Value, ModuleError> {
    let limit = f.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(ModuleError::BadRequest("limit must be 1 to 100"));
    }
    if driver_board && !matches!(c.role, crate::users::RoleType::Driver) {
        return Err(ModuleError::PermissionDenied);
    }
    let mut db = connection(&s).await?;
    let p = json!({"board":driver_board,"mine":f.mine.unwrap_or(false),"cursor":f.cursor,"limit":limit,"status":f.status,"from":f.from,"to":f.to});
    query(&mut db,r#"WITH filtered AS (SELECT id,data FROM scheduled_ride_view WHERE
 (CASE WHEN ($1->>'board')::boolean THEN CASE WHEN ($1->>'mine')::boolean THEN driver_id=$2 ELSE status='OPEN' AND scheduled_at>(now() AT TIME ZONE 'UTC') AND passenger_id<>$2 AND scheduled_driver_eligible($2,option_snapshot) END ELSE passenger_id=$2 END)
 AND ($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'from' IS NULL OR scheduled_at>=($1->>'from')::timestamptz AT TIME ZONE 'UTC') AND ($1->>'to' IS NULL OR scheduled_at<($1->>'to')::timestamptz AT TIME ZONE 'UTC')),
 page AS (SELECT * FROM filtered WHERE $1->>'cursor' IS NULL OR id<($1->>'cursor')::uuid ORDER BY id DESC LIMIT (($1->>'limit')::int+1)), visible AS (SELECT * FROM page ORDER BY id DESC LIMIT ($1->>'limit')::int)
 SELECT jsonb_build_object('items',COALESCE((SELECT jsonb_agg(data ORDER BY id DESC) FROM visible),'[]'::jsonb),'total',(SELECT count(*) FROM filtered),'nextCursor',CASE WHEN (SELECT count(*) FROM page)>($1->>'limit')::int THEN (SELECT id FROM visible ORDER BY id LIMIT 1) END) AS data"#,p,c.user_id).await
}
/// Authorize booking detail for its passenger, assigned driver, admin or an eligible driver browsing an open request.
pub async fn detail(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, ModuleError> {
    let mut db = connection(&s).await?;
    let p = json!({"user":c.user_id,"admin":matches!(c.role,crate::users::RoleType::Admin|crate::users::RoleType::Root),"driver":matches!(c.role,crate::users::RoleType::Driver)});
    query(&mut db,"SELECT data FROM scheduled_ride_view WHERE id=$2 AND (passenger_id=($1->>'user')::uuid OR driver_id=($1->>'user')::uuid OR ($1->>'admin')::boolean OR (($1->>'driver')::boolean AND status='OPEN' AND scheduled_at>(now() AT TIME ZONE 'UTC') AND scheduled_driver_eligible(($1->>'user')::uuid,option_snapshot)))",p,id).await
}
/// Lock and reserve through the common driver/admin path; the boolean identifies a new assignment for auditing.
pub(crate) async fn reserve_locked(
    db: &mut Connection<'_>,
    driver: Uuid,
    id: Uuid,
    expected_version: i32,
) -> Result<(Value, bool), ModuleError> {
    let before = detail_query(db, id).await?;
    let passenger: Uuid = serde_json::from_value(before["passengerId"].clone())?;
    if driver == passenger {
        return Err(ModuleError::PermissionDenied);
    }
    lock_users(db, passenger, Some(driver)).await?;
    let locked=query(db,"SELECT to_jsonb(s) AS data FROM scheduled_rides s WHERE id=$2 AND $1 IS NOT NULL FOR UPDATE",json!({}),id).await?;
    if locked["status"] == "RESERVED" && locked["driver_id"] == json!(driver) {
        return Ok((detail_query(db, id).await?, false));
    }
    if locked["status"] != "OPEN" {
        return Err(ModuleError::Conflict("Scheduled request is no longer open"));
    }
    if locked["version"] != json!(expected_version) {
        return Err(ModuleError::Conflict(
            "Booking changed; refresh before accepting",
        ));
    }
    let valid=query(db,"SELECT jsonb_build_object('valid',scheduled_at>(now() AT TIME ZONE 'UTC') AND scheduled_driver_eligible(($1->>'driver')::uuid,option_snapshot)) AS data FROM scheduled_rides WHERE id=$2",json!({"driver":driver}),id).await?;
    if valid["valid"] != true {
        return Err(ModuleError::PermissionDenied);
    }
    ensure_no_overlap(db, id, passenger, Some(driver)).await?;
    query(db,"UPDATE scheduled_rides SET driver_id=($1->>'driver')::uuid,status='RESERVED',version=version+1,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"driver":driver}),id).await?;
    Ok((detail_query(db, id).await?, true))
}
/// Reserve through the shared implementation while retaining the driver-facing booking response.
pub(crate) async fn accept_locked(
    db: &mut Connection<'_>,
    driver: Uuid,
    id: Uuid,
    expected_version: i32,
) -> Result<Value, ModuleError> {
    Ok(reserve_locked(db, driver, id, expected_version).await?.0)
}
/// Reserve a future journey without making the driver busy on the current live network.
pub async fn accept(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    expected_version: i32,
) -> Result<Value, ModuleError> {
    if !matches!(c.role, crate::users::RoleType::Driver) {
        return Err(ModuleError::PermissionDenied);
    }
    let mut db = connection(&s).await?;
    let value = db
        .transaction::<Value, ModuleError, _>(|db| {
            Box::pin(accept_locked(db, c.user_id, id, expected_version))
        })
        .await?;
    wake(&s);
    Ok(value)
}
/// Cancel a passenger booking or release a driver's reservation and immediately rebroadcast it.
pub async fn cancel(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: ScheduledCancellationDto,
    driver_cancel: bool,
) -> Result<Value, ModuleError> {
    if p.reason.trim().is_empty() || p.reason.len() > 2000 {
        return Err(ModuleError::BadRequest(
            "Provide a cancellation reason up to 2000 bytes",
        ));
    }
    let mut db = connection(&s).await?;
    let value=db.transaction::<Value,ModuleError,_>(|db|Box::pin(async {
  let v=detail_query(db,id).await?;let passenger=serde_json::from_value(v["passengerId"].clone())?;let driver:Option<Uuid>=serde_json::from_value(v["driverId"].clone())?;
  lock_users(db,passenger,driver).await?;
  let v=query(db,"SELECT to_jsonb(s) AS data FROM scheduled_rides s WHERE id=$2 AND $1 IS NOT NULL FOR UPDATE",json!({}),id).await?;
  if (driver_cancel&&v["driver_id"]!=json!(c.user_id))||(!driver_cancel&&v["passenger_id"]!=json!(c.user_id)){return Err(ModuleError::PermissionDenied);}
  if v["driver_id"]!=json!(driver){return Err(ModuleError::Conflict("Assignment changed; refresh the booking"));}
  if v["status"]=="ACTIVATED"{return Err(ModuleError::Conflict("Use the linked live ride cancellation endpoint after activation"));}
  if !matches!(v["status"].as_str(),Some("OPEN"|"RESERVED")){return Err(ModuleError::Conflict("Booking is already closed"));}
  query(db,"UPDATE scheduled_rides SET status=CASE WHEN ($1->>'driverCancel')::boolean AND scheduled_at>(now() AT TIME ZONE 'UTC') THEN 'OPEN' WHEN ($1->>'driverCancel')::boolean THEN 'EXPIRED' ELSE 'CANCELLED' END,driver_id=CASE WHEN ($1->>'driverCancel')::boolean THEN NULL ELSE driver_id END,cancellation_reason=$1->>'reason',version=version+1,next_broadcast_at=scheduled_next_broadcast(scheduled_at),reminder_sent=false,updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"driverCancel":driver_cancel,"reason":p.reason}),id).await?;
  detail_query(db,id).await
 })).await?;
    wake(&s);
    Ok(value)
}
/// Move pickup time while retaining the original fare; an assigned driver must accept the changed reservation again.
pub async fn reschedule(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: RescheduleRideDto,
) -> Result<Value, ModuleError> {
    validate_pickup(p.scheduled_at, Utc::now())?;
    let mut db = connection(&s).await?;
    let value=db.transaction::<Value,ModuleError,_>(|db|Box::pin(async {
  let v=detail_query(db,id).await?;let passenger:Uuid=serde_json::from_value(v["passengerId"].clone())?;let driver=serde_json::from_value(v["driverId"].clone())?;
  if passenger!=c.user_id{return Err(ModuleError::PermissionDenied);}
  lock_users(db,passenger,driver).await?;timezone_allowed(db,&p.timezone).await?;
  let v=query(db,"SELECT to_jsonb(s) AS data FROM scheduled_rides s WHERE id=$2 AND $1 IS NOT NULL FOR UPDATE",json!({}),id).await?;
  if v["driver_id"]!=json!(driver){return Err(ModuleError::Conflict("Assignment changed; refresh the booking"));}
  if !matches!(v["status"].as_str(),Some("OPEN"|"RESERVED")){return Err(ModuleError::Conflict("Only open or reserved bookings can be rescheduled"));}
  query(db,"UPDATE scheduled_rides SET scheduled_at=($1->>'at')::timestamptz AT TIME ZONE 'UTC',timezone=$1->>'timezone',status='OPEN',driver_id=NULL,version=version+1,reminder_sent=false,next_broadcast_at=($1->>'at')::timestamptz AT TIME ZONE 'UTC'-interval '30 minutes',updated_at=(now() AT TIME ZONE 'UTC') WHERE id=$2 RETURNING '{}'::jsonb AS data",json!({"at":p.scheduled_at,"timezone":p.timezone}),id).await?;
  ensure_no_overlap(db,id,passenger,None).await?;detail_query(db,id).await
 })).await?;
    wake(&s);
    Ok(value)
}
/// Wake the mail worker without awaiting a bounded channel; persisted jobs remain recoverable if it is full.
pub(crate) fn wake(s: &AppState) {
    let _ = s
        .mailer
        .try_send(crate::mailer::types::MailerEvent::DrainOutbox);
}
