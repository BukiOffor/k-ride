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

/// Manually match an unassigned ride to an eligible driver.
pub async fn match_driver(
    s: Arc<AppState>,
    c: Claims,
    ride_id: Uuid,
    driver_id: Uuid,
) -> Result<Value, AdminError> {
    require_admin(&c)?;
    let mut db = connection(&s).await?;

    // 1. Verify driver is an active driver and not currently on an active trip
    let driver_status = query(
        &mut db,
        "SELECT jsonb_build_object(\
            'exists', EXISTS(SELECT 1 FROM users WHERE id=$2 AND role='Driver' AND is_active),\
            'approved', EXISTS(SELECT 1 FROM driver_profiles dp JOIN cars c ON c.id=dp.car_id WHERE dp.user_id=$2 AND dp.status='APPROVED' AND c.is_verified),\
            'onTrip', EXISTS(SELECT 1 FROM rides WHERE driver_id=$2 AND status IN ('DRIVER_ASSIGNED','DRIVER_EN_ROUTE','DRIVER_ARRIVING','DRIVER_ARRIVED','IN_PROGRESS','PAUSED'))\
        ) AS data WHERE $1 IS NOT NULL",
        json!({}),
        driver_id,
    )
    .await?;

    if !driver_status["exists"].as_bool().unwrap_or(false) {
        return Err(bad("Driver not found or account is not an active driver"));
    }
    if !driver_status["approved"].as_bool().unwrap_or(false) {
        return Err(bad("Driver profile and vehicle must be approved"));
    }
    if driver_status["onTrip"].as_bool().unwrap_or(false) {
        return Err(conflict(
            "Driver is already assigned to another active ride",
        ));
    }

    // 2. Verify ride exists and is in an unassigned matchable state
    let ride_state = query(
        &mut db,
        "SELECT jsonb_build_object(\
            'exists', count(*) > 0,\
            'driverId', (SELECT driver_id FROM rides WHERE id=$2),\
            'status', (SELECT status FROM rides WHERE id=$2)\
        ) AS data FROM rides WHERE id=$2 AND $1 IS NOT NULL",
        json!({}),
        ride_id,
    )
    .await?;

    if !ride_state["exists"].as_bool().unwrap_or(false) {
        return Err(missing());
    }
    if !ride_state["driverId"].is_null() {
        return Err(conflict("Ride has already been matched to a driver"));
    }
    let status_str = ride_state["status"].as_str().unwrap_or("");
    if !matches!(status_str, "REQUESTED" | "SEARCHING" | "NO_DRIVERS") {
        return Err(conflict(
            "Ride cannot be matched because it is not in an unmatched state (REQUESTED, SEARCHING, or NO_DRIVERS)",
        ));
    }

    // 3. Perform atomic update: assign driver, update availability, withdraw other offers, insert accepted offer & audit
    let offer_id = Uuid::now_v7();
    let assign_params = json!({
        "driverId": driver_id,
        "offerId": offer_id,
    });

    let assigned = db
        .transaction::<Value, AdminError, _>(|db| {
            Box::pin(async move {
                // Update ride to DRIVER_ASSIGNED
                let update_res = query(
                    db,
                    "UPDATE rides \
                     SET driver_id = ($1->>'driverId')::uuid, \
                         status = 'DRIVER_ASSIGNED', \
                         assigned_at = (now() AT TIME ZONE 'UTC'), \
                         version = version + 1, \
                         updated_at = (now() AT TIME ZONE 'UTC') \
                     WHERE id = $2 \
                       AND driver_id IS NULL \
                       AND status IN ('REQUESTED', 'SEARCHING', 'NO_DRIVERS') \
                     RETURNING jsonb_build_object('id', id, 'passengerId', passenger_id, 'version', version) AS data",
                    assign_params.clone(),
                    ride_id,
                )
                .await
                .map_err(|e| match e {
                    AdminError(StatusCode::NOT_FOUND, _, _) => {
                        conflict("Ride has already been matched or is no longer available")
                    }
                    other => other,
                })?;

                // Set driver availability to ON_TRIP
                let _ = query(
                    db,
                    "INSERT INTO driver_availability (driver_id, status, updated_at) \
                     VALUES ($2, 'ON_TRIP', (now() AT TIME ZONE 'UTC')) \
                     ON CONFLICT (driver_id) DO UPDATE \
                     SET status = 'ON_TRIP', updated_at = (now() AT TIME ZONE 'UTC') \
                     RETURNING jsonb_build_object('status', status) AS data",
                    json!({}),
                    driver_id,
                )
                .await;

                // Withdraw any open offers for other drivers and capture their IDs
                let withdrawn = query(
                    db,
                    "WITH w AS ( \
                         UPDATE ride_offers \
                         SET status = 'withdrawn' \
                         WHERE ride_id = $2 AND status = 'offered' \
                         RETURNING id, driver_id \
                     ) SELECT COALESCE(jsonb_agg(jsonb_build_object('id', id, 'driverId', driver_id)), '[]'::jsonb) AS data FROM w WHERE $1 IS NOT NULL",
                    json!({}),
                    ride_id,
                )
                .await
                .unwrap_or_else(|_| json!([]));

                // Insert accepted offer record for the matched driver
                let _ = query(
                    db,
                    "INSERT INTO ride_offers (id, ride_id, driver_id, dispatch_round, status, created_at, expires_at, responded_at) \
                     SELECT \
                       ($1->>'offerId')::uuid, \
                       id, \
                       ($1->>'driverId')::uuid, \
                       dispatch_round, \
                       'accepted', \
                       (now() AT TIME ZONE 'UTC'), \
                       (now() AT TIME ZONE 'UTC') + interval '20 seconds', \
                       (now() AT TIME ZONE 'UTC') \
                     FROM rides WHERE id = $2 \
                     RETURNING jsonb_build_object('id', id) AS data",
                    assign_params.clone(),
                    ride_id,
                )
                .await;

                // Audit log
                audit(
                    db,
                    c.user_id,
                    ride_id,
                    "ride.driver_assigned",
                    json!({"driverId": driver_id, "manual": true}),
                )
                .await?;

                Ok(json!({
                    "ride": update_res,
                    "withdrawn": withdrawn,
                }))
            })
        })
        .await?;

    // 4. Update memory registry
    s.ws_registry
        .set_driver_active_ride(driver_id, Some(ride_id));
    s.ws_registry.set_driver_online(driver_id, false);

    // 5. Broadcast to other drivers whose offers were withdrawn
    if let Some(withdrawn_list) = assigned["withdrawn"].as_array() {
        for w in withdrawn_list {
            if let (Some(w_id), Some(w_driver_id)) = (w["id"].as_str(), w["driverId"].as_str()) {
                if let (Ok(offer_u), Ok(driver_u)) =
                    (Uuid::parse_str(w_id), Uuid::parse_str(w_driver_id))
                {
                    let withdrawn_evt = crate::vrp::realtime::WsEventFrame {
                        id: format!("evt_{}", Uuid::now_v7()),
                        ride_id: Some(ride_id),
                        seq: 1,
                        ride_version: None,
                        occurred_at: chrono::Utc::now().to_rfc3339(),
                        payload: json!({
                            "type": "driver.request.withdrawn",
                            "offerId": offer_u,
                        }),
                    };
                    let _ = s
                        .ws_registry
                        .send_to_driver(
                            &driver_u,
                            crate::vrp::realtime::OutgoingWsMessage::Event {
                                event: withdrawn_evt,
                            },
                        )
                        .await;
                }
            }
        }
    }

    // 6. Broadcast ride.updated event to ride channel and directly to driver mailbox
    use diesel::{ExpressionMethods, QueryDsl};
    let updated_ride: Result<crate::vrp::models::rides::Ride, _> =
        diesel_async::RunQueryDsl::first(
            crate::schema::rides::table.filter(crate::schema::rides::id.eq(ride_id)),
            &mut db,
        )
        .await;

    if let Ok(ride_model) = updated_ride {
        if let Ok(proj) = crate::vrp::services::rides::build_ride_projection(
            &mut db,
            &ride_model,
            ride_model.passenger_id,
            false,
        )
        .await
        {
            let evt = crate::vrp::realtime::WsEventFrame {
                id: format!("evt_{}", Uuid::now_v7()),
                ride_id: Some(ride_id),
                seq: 1,
                ride_version: Some(ride_model.version),
                occurred_at: chrono::Utc::now().to_rfc3339(),
                payload: json!({
                    "type": "ride.updated",
                    "ride": proj,
                }),
            };
            s.ws_registry.broadcast_ride_event(ride_id, evt.clone());
            let _ = s
                .ws_registry
                .send_to_driver(
                    &driver_id,
                    crate::vrp::realtime::OutgoingWsMessage::Event { event: evt },
                )
                .await;
        }
    }

    drop(db);

    // 7. Return refreshed ride details
    ride(s, c, ride_id).await
}
