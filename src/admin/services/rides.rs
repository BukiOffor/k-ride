//! Business logic and database operations for rides.
use super::*;

pub(crate) const RIDE_FILTER: &str = "($1->>'state' IS DISTINCT FROM 'active' OR status IN ('SEARCHING','DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED')) AND ($1->>'status' IS NULL OR status=$1->>'status') AND ($1->>'sos' IS NULL OR ((data->'sos'->>'status') IN ('sent','acknowledged','responding'))=($1->>'sos')::boolean) AND ($1->>'q' IS NULL OR concat_ws(' ',data->>'reference',data->'driver'->>'displayName',data->'passengers',data->'destination'->>'label',data->'destination'->>'name') ILIKE '%' || ($1->>'q') || '%')";

/// List network rides with server-side filtering, pagination and filter counts.
pub async fn rides(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;
    let mut result=query(&mut db,&page_sql("(SELECT id,created_at,status,driver_id,passenger_id,data-'route'-'fare'-'payment'-'option'-'rating'-'cancellation'-'progress'-'approach'-'timestamps' AS data FROM admin_rides_view) summaries",RIDE_FILTER),f.params()?,c.user_id).await?;
    result["counts"]=query(&mut db,"SELECT jsonb_build_object('all',count(*),'sos',count(*) FILTER(WHERE data->'sos'->>'status' IN ('sent','acknowledged','responding')),'inProgress',count(*) FILTER(WHERE status IN ('IN_PROGRESS','PAUSED')),'searching',count(*) FILTER(WHERE status IN ('REQUESTED','SEARCHING'))) AS data FROM admin_rides_view WHERE ($1->>'state' IS DISTINCT FROM 'active' OR status IN ('SEARCHING','DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED')) AND $2 IS NOT NULL",f.params()?,c.user_id).await?;
    Ok(result)
}

/// Load one network ride, its timeline and incident-gated contact details.
pub async fn ride(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
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
    Ok(result)
}
