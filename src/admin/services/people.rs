//! Business logic and database operations for people.
use super::*;

pub(crate) const PEOPLE_FILTER: &str = "($1->>'q' IS NULL OR concat_ws(' ',data->>'displayName',data->>'email',data->'vehicle'->>'plate') ILIKE '%' || ($1->>'q') || '%') AND ($1->>'track' IS NULL OR track=$1->>'track' OR track='Both') AND ($1->>'availability' IS NULL OR upper(data->>'availability')=upper($1->>'availability')) AND ($1->>'verification' IS NULL OR upper(verification_status)=upper($1->>'verification')) AND ($1->>'active' IS NULL OR is_active=($1->>'active')::boolean) AND ($1->>'flagged' IS NULL OR (data->>'flagged')::boolean=($1->>'flagged')::boolean) AND ($1->>'membership' IS NULL OR data->>'membershipStatus'=$1->>'membership') AND ($1->>'hasTrustedContact' IS NULL OR (COALESCE((data->>'trustedContactCount')::int,0)>0)=($1->>'hasTrustedContact')::boolean)";

/// Build a filtered driver or passenger roster and its filter counts.
pub(crate) async fn people(
    s: &AppState,
    c: &Claims,
    f: Filters,
    role: &str,
) -> Result<Value, AdminError> {
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
    Ok(result)
}

/// List drivers with vehicle, verification and account oversight summaries.
pub async fn drivers(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    people(&s, &c, f, "Driver").await
}

/// List passengers with membership, ride and trusted-contact summaries.
pub async fn passengers(s: Arc<AppState>, c: Claims, f: Filters) -> Result<Value, AdminError> {
    people(&s, &c, f, "User").await
}

/// Build a role-specific person record with statistics and oversight details.
pub(crate) async fn person(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    role: &str,
) -> Result<Value, AdminError> {
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
    Ok(v)
}

/// Load a driver record, activity statistics, affiliation and vehicle information.
pub async fn driver(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    person(&s, &c, id, "Driver").await
}

/// Load a passenger record, ride statistics and trusted-contact summaries.
pub async fn passenger(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    person(&s, &c, id, "User").await
}

/// Page through rides belonging to one driver or passenger.
pub(crate) async fn history(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    f: Filters,
    column: &str,
) -> Result<Value, AdminError> {
    require_admin(c)?;
    let mut db = connection(s).await?;
    Ok(query(
        &mut db,
        &page_sql("admin_rides_view", &format!("{column}=$2")),
        f.params()?,
        id,
    )
    .await?)
}

/// Return only the selected driver’s ride history.
pub async fn driver_rides(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    f: Filters,
) -> Result<Value, AdminError> {
    history(&s, &c, id, f, "driver_id").await
}

/// Return only the selected passenger’s ride history.
pub async fn passenger_rides(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    f: Filters,
) -> Result<Value, AdminError> {
    history(&s, &c, id, f, "passenger_id").await
}

/// Create or update a manual driver quality flag with a required reason.
pub async fn flag(s: Arc<AppState>, c: Claims, id: Uuid, p: Reason) -> Result<Value, AdminError> {
    text(&p.reason)?;
    Ok(mutate(&s,&c,id,"quality.flagged","INSERT INTO admin_quality_flags(id,subject_id,reason) SELECT ($1->>'id')::uuid,id,$1->>'reason' FROM users WHERE id=$2 AND role='Driver' ON CONFLICT(subject_id) WHERE cleared_at IS NULL DO UPDATE SET reason=EXCLUDED.reason RETURNING jsonb_build_object('id',id,'reason',reason) AS data",json!({"id":Uuid::now_v7(),"reason":p.reason})).await?)
}

/// Clear the driver’s open quality flag without deleting ratings.
pub async fn unflag(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    Ok(mutate(&s,&c,id,"quality.cleared","UPDATE admin_quality_flags SET cleared_at=(now() AT TIME ZONE 'UTC') WHERE subject_id=$2 AND cleared_at IS NULL AND $1 IS NOT NULL RETURNING jsonb_build_object('id',id) AS data",json!({})).await?)
}

/// Deactivate a driver, withdraw offers and remove them from live dispatch.
pub async fn deactivate(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: Reason,
) -> Result<Value, AdminError> {
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
    Ok(v)
}

/// Block new ride requests for a passenger while preserving ongoing trips.
pub async fn restrict(
    s: Arc<AppState>,
    c: Claims,
    id: Uuid,
    p: Reason,
) -> Result<Value, AdminError> {
    text(&p.reason)?;
    restriction(&s, &c, id, true, Some(p.reason)).await
}

/// Lift the passenger’s ride-request restriction.
pub async fn unrestrict(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    restriction(&s, &c, id, false, None).await
}

/// Persist a passenger restriction change and its audit entry together.
pub(crate) async fn restriction(
    s: &AppState,
    c: &Claims,
    id: Uuid,
    value: bool,
    reason: Option<String>,
) -> Result<Value, AdminError> {
    Ok(mutate(s,c,id,"passenger.restriction_changed","INSERT INTO admin_user_state(user_id,restricted,restriction_reason) SELECT id,($1->>'restricted')::boolean,$1->>'reason' FROM users WHERE id=$2 AND role='User' ON CONFLICT(user_id) DO UPDATE SET restricted=EXCLUDED.restricted,restriction_reason=EXCLUDED.restriction_reason,updated_at=(now() AT TIME ZONE 'UTC') RETURNING jsonb_build_object('userId',user_id,'restricted',restricted) AS data",json!({"restricted":value,"reason":reason})).await?)
}

/// Mark a passenger membership check as pending provider verification.
pub async fn membership_check(s: Arc<AppState>, c: Claims, id: Uuid) -> Result<Value, AdminError> {
    Ok(mutate(&s,&c,id,"membership.check_requested","INSERT INTO admin_user_state(user_id,membership_status) SELECT id,'CHECKING' FROM users WHERE id=$2 AND role='User' AND $1 IS NOT NULL ON CONFLICT(user_id) DO UPDATE SET membership_status='CHECKING',updated_at=(now() AT TIME ZONE 'UTC') RETURNING jsonb_build_object('userId',user_id,'membershipStatus',membership_status) AS data",json!({})).await?)
}

/// Check whether a passenger is blocked from creating new ride requests.
pub async fn requests_restricted(db: &mut Connection<'_>, id: Uuid) -> Result<bool, ModuleError> {
    let row=diesel::sql_query("SELECT jsonb_build_object('restricted',COALESCE((SELECT restricted FROM admin_user_state WHERE user_id=$1),false)) AS data").bind::<SqlUuid,_>(id).get_result::<Row>(db).await?;
    Ok(row.data["restricted"].as_bool().unwrap_or(false))
}

/// List every driver currently live in the dispatch index with their latest position.
///
/// Positions come from Redis (in-memory, updated on every ping); identity, heading/speed
/// and current-ride details are enriched from Postgres in a single query.
pub async fn live_drivers(s: Arc<AppState>, c: Claims) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut dispatcher = (*s.dispatcher).clone();
    let live = dispatcher.list_live_drivers().await.map_err(|e| {
        tracing::error!(error=%e,"Failed to read live drivers from dispatch index");
        AdminError(
            StatusCode::SERVICE_UNAVAILABLE,
            "dispatch_unavailable",
            "Live driver index unavailable".into(),
        )
    })?;

    let drivers: Vec<Value> = live
        .into_iter()
        .filter(|(d, _)| Uuid::parse_str(&d.driver_id).is_ok())
        .map(|(d, last_seen)| {
            let last_seen_at = chrono::DateTime::from_timestamp_millis((last_seen * 1000.0) as i64)
                .map(|t| t.to_rfc3339());
            json!({"driverId":d.driver_id,"lat":d.lat,"lng":d.lng,"availableSeats":d.available_seats,"lastSeenAt":last_seen_at})
        })
        .collect();

    if drivers.is_empty() {
        return Ok(json!({"items":[],"total":0}));
    }

    let mut db = connection(&s).await?;
    query(
        &mut db,
        "SELECT jsonb_build_object('items',COALESCE(jsonb_agg(jsonb_build_object(\
            'driverId',u.id,\
            'displayName',trim(concat_ws(' ',u.first_name,u.last_name)),\
            'phone',u.phone,\
            'lat',(d->>'lat')::float8,\
            'lng',(d->>'lng')::float8,\
            'heading',a.heading,\
            'speedMps',a.speed_mps,\
            'availableSeats',(d->>'availableSeats')::int,\
            'lastSeenAt',d->>'lastSeenAt',\
            'currentRide',(SELECT jsonb_build_object('rideId',r.id,'status',r.status) FROM rides r WHERE r.driver_id=u.id AND r.status IN ('DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED') ORDER BY r.created_at DESC LIMIT 1)\
        ) ORDER BY u.first_name,u.last_name),'[]'::jsonb),'total',count(u.id)) AS data \
        FROM jsonb_array_elements($1->'drivers') d \
        JOIN users u ON u.id=(d->>'driverId')::uuid AND u.is_active \
        LEFT JOIN driver_availability a ON a.driver_id=u.id \
        WHERE $2 IS NOT NULL",
        json!({"drivers":drivers}),
        c.user_id,
    )
    .await
}
